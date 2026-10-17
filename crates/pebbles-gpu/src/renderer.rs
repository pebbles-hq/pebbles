//! One [`Renderer`] per device: the eager resource set (Globals, arenas, layouts, pipeline
//! cache, glyph atlas).

use std::collections::HashMap;

use kurbo::Affine;
use wgpu::util::DeviceExt;

use pebbles_render::paint::Scene;

use super::context::{DeviceId, GpuDevice};
use super::error::GpuError;
use super::execute::encode::{encode, resolve_and_upload};
use super::execute::pipelines::{PipelineCache, ShaderModules};
use super::execute::resources::bind_groups::BindGroupLayouts;
use super::execute::resources::buffers::FrameArenas;
use super::execute::resources::globals::Globals;
use super::execute::resources::glyph_atlas::GlyphAtlas;
use super::frame_targets::create_msaa;
use super::gpu_types::QuadInstance;
use super::prepare::{PrepareInput, PrepareScratch, VERTEX_LIMIT_NO_FULL_INDEX, prepare};
use super::stats::{FrameStats, Unsupported, WarnOnce};

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
    pub(crate) glyph_atlas: GlyphAtlas,
    pub(crate) glyph_instances: Vec<QuadInstance>,
    pub(crate) image_cache: HashMap<u64, (wgpu::Texture, wgpu::TextureView, wgpu::BindGroup)>,
    pub(crate) scratch: PrepareScratch,
    stats: FrameStats,
    warn_once: WarnOnce,
    vertex_limit: u32,
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
        let glyph_atlas = GlyphAtlas::new(device, &layouts.glyph);
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
            glyph_atlas,
            glyph_instances: Vec::new(),
            image_cache: HashMap::new(),
            scratch: PrepareScratch::default(),
            stats: FrameStats::default(),
            warn_once: WarnOnce::default(),
            vertex_limit: if gpu
                .adapter()
                .get_downlevel_capabilities()
                .flags
                .contains(wgpu::DownlevelFlags::FULL_DRAW_INDEX_UINT32)
            {
                u32::MAX
            } else {
                VERTEX_LIMIT_NO_FULL_INDEX
            },
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

        let mut stats = FrameStats::default();
        let input = PrepareInput {
            target_size: size,
            root_transform: Affine::IDENTITY,
            vertex_limit: self.vertex_limit,
        };
        prepare(scene, &input, &mut self.scratch, &mut stats);

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
        self.glyph_atlas.begin_frame();
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
        self.glyph_atlas.end_frame();
        self.end_frame(&stats);
        self.stats = stats;
        Ok(stats)
    }
}

impl Renderer {
    /// After the last submit: log each fallback kind the first time it occurs (06 §6.3).
    fn end_frame(&mut self, stats: &FrameStats) {
        for kind in Unsupported::ALL {
            self.warn_once.report(kind, stats.unsupported[kind as usize]);
        }
    }

    pub(crate) fn globals_bg(&self) -> &wgpu::BindGroup {
        &self.globals_bg
    }
}
