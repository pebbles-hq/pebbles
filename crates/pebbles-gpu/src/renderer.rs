//! One [`Renderer`] per device: the eager resource set (Globals, arenas, layouts, pipeline
//! cache) plus the lazy pre-C09 glyph system.

use std::collections::HashMap;

use cosmic_text::{FontSystem, SwashCache};
use kurbo::Affine;
use wgpu::util::DeviceExt;

use pebbles_render::paint::Scene;

use super::context::{DeviceId, GpuDevice};
use super::error::GpuError;
use super::execute::encode::{encode, resolve_and_upload};
use super::execute::pipelines::{PipelineCache, ShaderModules};
use super::execute::resources::Atlas;
use super::execute::resources::bind_groups::BindGroupLayouts;
use super::execute::resources::buffers::FrameArenas;
use super::execute::resources::globals::Globals;
use super::frame_targets::create_msaa;
use super::prepare::{PrepareInput, PrepareScratch, prepare};
use super::stats::FrameStats;

/// Per-frame render parameters.
pub struct RenderParams {
    /// The clear color.
    pub base_color: peniko::Color,
    /// Target width in physical pixels.
    pub width: u32,
    /// Target height in physical pixels.
    pub height: u32,
}

/// Pipelines + caches for ONE device. Shared by every surface/target on that device.
pub struct Renderer {
    pub(crate) sample_count: u32,
    pub(crate) shaders: ShaderModules,
    pub(crate) layouts: BindGroupLayouts,
    pub(crate) pipelines: PipelineCache,
    globals_buffer: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    pub(crate) arenas: FrameArenas,
    pub(crate) image_sampler: wgpu::Sampler,
    pub(crate) image_index_buffer: wgpu::Buffer,
    msaa: Option<(wgpu::TextureFormat, u32, u32, wgpu::Texture)>,
    pub(crate) fonts: Option<FontSystem>,
    pub(crate) swash: Option<SwashCache>,
    pub(crate) font_ids: HashMap<(u64, u32), fontdb::ID>,
    pub(crate) atlas: Option<Atlas>,
    pub(crate) color_atlas: Option<Atlas>,
    glyph_mask_bg: Option<wgpu::BindGroup>,
    glyph_color_bg: Option<wgpu::BindGroup>,
    pub(crate) image_cache: HashMap<u64, (wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)>,
    pub(crate) scratch: PrepareScratch,
    stats: FrameStats,
}

impl Renderer {
    /// Build the eager resource set (Globals, arenas, layouts, shader modules, samplers).
    /// The pre-C09 glyph system and image cache stay lazy.
    pub fn new(gpu: &GpuDevice, _id: DeviceId) -> Self {
        let device = gpu.device();
        let globals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pebbles.gpu.buffer.globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layouts = BindGroupLayouts::new(device);
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pebbles.gpu.bind.globals"),
            layout: &layouts.globals,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &globals_buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(size_of::<Globals>() as u64),
                }),
            }],
        });
        let image_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("pebbles.gpu.image.sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let image_index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pebbles.gpu.image.indices"),
            contents: bytemuck::cast_slice(&[0u32, 1, 2, 0, 2, 3]),
            usage: wgpu::BufferUsages::INDEX,
        });
        Renderer {
            sample_count: gpu.caps().msaa_samples,
            shaders: ShaderModules::new(device),
            layouts,
            pipelines: PipelineCache::new(),
            globals_buffer,
            globals_bg,
            arenas: FrameArenas::new(),
            image_sampler,
            image_index_buffer,
            msaa: None,
            fonts: None,
            swash: None,
            font_ids: HashMap::new(),
            atlas: None,
            color_atlas: None,
            glyph_mask_bg: None,
            glyph_color_bg: None,
            image_cache: HashMap::new(),
            scratch: PrepareScratch::default(),
            stats: FrameStats::default(),
        }
    }

    /// The `FrameStats` of the last render.
    pub fn last_stats(&self) -> FrameStats {
        self.stats
    }

    /// Render `scene` into `target` (a view of a `format` texture of `params` size).
    pub fn render_to_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        params: &RenderParams,
    ) -> Result<FrameStats, GpuError> {
        self.render_offscreen(
            device,
            queue,
            scene,
            target,
            params.base_color,
            (params.width, params.height),
            format,
        )
    }

    /// Render `scene` into `target`, clearing to `base_color` first.
    #[allow(clippy::too_many_arguments)]
    pub fn render_offscreen(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        target: &wgpu::TextureView,
        base_color: peniko::Color,
        size: (u32, u32),
        format: wgpu::TextureFormat,
    ) -> Result<FrameStats, GpuError> {
        let size = (size.0.max(1), size.1.max(1));
        let msaa = match &self.msaa {
            Some((f, w, h, texture)) if *f == format && (*w, *h) == size => texture.clone(),
            _ => {
                let texture = create_msaa(device, format, size.0, size.1, self.sample_count);
                self.msaa = Some((format, size.0, size.1, texture.clone()));
                texture
            }
        };

        let input = PrepareInput { target_size: size, root_transform: Affine::IDENTITY };
        prepare(scene, &input, &mut self.scratch);

        let mut stats = FrameStats::default();
        stats.ops = self.scratch.plan.stats.ops;
        stats.cmds = u32::try_from(self.scratch.plan.cmds.len()).unwrap_or(u32::MAX);
        stats.culled = self.scratch.plan.stats.culled;

        let globals = Globals {
            target_size: [size.0 as f32, size.1 as f32],
            target_origin: [0.0, 0.0],
            inv_target_size: [1.0 / size.0 as f32, 1.0 / size.1 as f32],
            color_flags: 0,
            _pad0: 0,
        };
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));

        self.arenas.solid_vertices.upload(
            device,
            queue,
            bytemuck::cast_slice(&self.scratch.plan.solid_vertices),
            &mut stats,
        );
        self.arenas.solid_indices.upload(
            device,
            queue,
            bytemuck::cast_slice(&self.scratch.plan.solid_indices),
            &mut stats,
        );
        self.arenas.shadow_instances.upload(
            device,
            queue,
            bytemuck::cast_slice(&self.scratch.plan.shadows),
            &mut stats,
        );

        self.pipelines.prewarm(device, &self.shaders, &self.layouts, format, self.sample_count, &mut stats);
        let frame = resolve_and_upload(self, device, queue, scene, &mut stats);

        let clear = super::prepare::color::clear_color(base_color);
        let mut encoder = device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("pebbles.gpu.encoder") });
        {
            let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pebbles.gpu.pass.root"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    depth_slice: None,
                    resolve_target: Some(target),
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            encode(self, &frame, &mut pass, format, &mut stats);
            stats.render_passes += 1;
        }
        queue.submit([encoder.finish()]);
        self.scratch.plan.release_refs();
        self.stats = stats;
        Ok(stats)
    }
}

impl Renderer {
    pub(crate) fn globals_bg(&self) -> &wgpu::BindGroup {
        &self.globals_bg
    }

    pub(crate) fn glyph_bg(&self, color: bool) -> Option<&wgpu::BindGroup> {
        if color { self.glyph_color_bg.as_ref() } else { self.glyph_mask_bg.as_ref() }
    }

    pub(crate) fn set_glyph_bg(&mut self, color: bool, bg: wgpu::BindGroup) {
        if color {
            self.glyph_color_bg = Some(bg);
        } else {
            self.glyph_mask_bg = Some(bg);
        }
    }
}
