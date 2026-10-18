//! One [`Renderer`] per device: the eager resource set (Globals, arenas, layouts, pipeline
//! cache, glyph atlas).

use pebbles_render::paint::Scene;

use super::context::{DeviceId, GpuDevice};
use super::error::GpuError;
use super::execute::encode::{encode, resolve_and_upload};
use super::execute::pipelines::{PipelineCache, ShaderModules};
use super::execute::resources::bind_groups::BindGroupLayouts;
use super::execute::resources::buffers::FrameArenas;
use super::execute::resources::globals::Globals;
use super::execute::resources::glyph_atlas::GlyphAtlas;
use super::execute::resources::image_cache::ImageCache;
use super::execute::targets::{TargetCheck, check_target_size};
use super::frame_targets::{FrameParams, RenderTarget, TargetKey};
use super::gpu_types::QuadInstance;
use super::limits::RendererLimits;
use super::prepare::{PrepareInput, PrepareScratch, VERTEX_LIMIT_NO_FULL_INDEX, prepare};
use super::stats::{FrameStats, Unsupported, WarnOnce};

/// Pipelines + caches for ONE device. Shared by every surface/target on that device.
pub struct Renderer {
    pub(crate) sample_count: u32,
    adapter: wgpu::Adapter,
    msaa: super::frame_targets::MsaaCache,
    pub(crate) shaders: ShaderModules,
    pub(crate) layouts: BindGroupLayouts,
    pub(crate) pipelines: PipelineCache,
    globals_buffer: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    pub(crate) arenas: FrameArenas,
    device: wgpu::Device,
    queue: wgpu::Queue,
    health: std::sync::Arc<super::context::DeviceHealth>,
    pub(crate) glyph_atlas: GlyphAtlas,
    pub(crate) glyph_instances: Vec<QuadInstance>,
    pub(crate) image_cache: ImageCache,
    pub(crate) image_instances: Vec<QuadInstance>,
    pub(crate) scratch: PrepareScratch,
    stats: FrameStats,
    warn_once: WarnOnce,
    vertex_limit: u32,
    limits: RendererLimits,
}

impl Renderer {
    /// The MSAA sample count negotiated for the last target format (02 §5.4; 1 or 4).
    pub fn msaa_sample_count(&self) -> u32 {
        self.sample_count
    }

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
        Renderer {
            sample_count: gpu.caps().msaa_samples,
            adapter: gpu.adapter().clone(),
            msaa: super::frame_targets::MsaaCache::default(),
            shaders: ShaderModules::new(device),
            layouts,
            pipelines: PipelineCache::new(),
            globals_buffer,
            globals_bg,
            arenas: FrameArenas::new(),
            device: device.clone(),
            queue: gpu.queue().clone(),
            health: std::sync::Arc::clone(gpu.health()),
            glyph_atlas,
            glyph_instances: Vec::new(),
            image_cache: ImageCache::new(device),
            image_instances: Vec::new(),
            limits: RendererLimits::from_device_limits(&device.limits()),
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

    /// The hard caps derived from this Renderer's device (11 §2.5).
    pub fn limits(&self) -> &RendererLimits {
        &self.limits
    }

    /// Debug switch (R10): `false` draws rect / rounded-rect / circle fills through the tessellated
    /// path instead of Tier A analytic quads. Tests and the render-check tool only.
    #[doc(hidden)]
    pub fn set_sdf_enabled(&mut self, enabled: bool) {
        self.scratch.sdf_disabled = !enabled;
    }

    /// The `FrameStats` of the last render.
    pub fn last_stats(&self) -> FrameStats {
        self.stats
    }

    /// Prepare → resolve → upload → encode → submit → end-of-frame, into `target`.
    ///
    /// A zero-size target is a no-op (`Ok` with empty stats, no encoder; S07). Errors:
    /// `DeviceLost` when the device's lost callback fired; `InvalidTargetSize` when a side is
    /// above the device limit.
    pub fn render(
        &mut self,
        target: RenderTarget<'_>,
        scene: &Scene,
        params: &FrameParams,
    ) -> Result<FrameStats, GpuError> {
        if self.health.is_lost() {
            return Err(GpuError::DeviceLost);
        }
        let device = self.device.clone();
        let queue = self.queue.clone();
        let (device, queue) = (&device, &queue);
        let size = target.size();
        match check_target_size(size.0, size.1, &self.limits) {
            TargetCheck::Skip => return Ok(FrameStats::default()),
            TargetCheck::TooLarge { width, height, max } => {
                return Err(GpuError::InvalidTargetSize { width, height, max });
            }
            TargetCheck::Ok => {}
        }
        let format = target.format();
        let mut stats = FrameStats::default();
        self.sample_count = self.msaa.get(&self.adapter, format, None);
        let key = TargetKey { width: size.0, height: size.1, format, sample_count: self.sample_count };
        let msaa = target.targets.msaa_view(device, key, &mut stats).cloned();

        let input = PrepareInput {
            target_size: size,
            root_transform: params.root_transform,
            vertex_limit: self.vertex_limit,
        };
        prepare(scene, &input, &self.limits, &mut self.scratch, &mut stats);
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
        self.arenas.shape_instances.upload(
            device,
            queue,
            bytemuck::cast_slice(&self.scratch.plan.shapes),
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
        self.image_cache.begin_frame();
        resolve_and_upload(self, device, queue, &mut stats);

        let clear = super::prepare::color::clear_color(params.base_color);
        let mut encoder = device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("pebbles.gpu.encoder") });
        {
            // Single-sampled devices draw straight into the target (no resolve).
            let (view, resolve_target) = match &msaa {
                Some(m) => (m, Some(target.view)),
                None => (target.view, None),
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pebbles.gpu.pass.root"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(clear), store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            encode(self, &mut pass, format, &mut stats);
            stats.render_passes += 1;
        }
        queue.submit([encoder.finish()]);
        self.scratch.plan.release_refs();
        self.glyph_atlas.end_frame();
        self.image_cache.end_frame(&mut stats);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::tests::gpu_or_skip;
    use crate::frame_targets::FrameTargets;
    use crate::init::GpuInit;

    #[test]
    #[ignore = "gpu"]
    fn render_after_loss_returns_device_lost() {
        let Some((ctx, id)) = gpu_or_skip(&GpuInit::headless()) else { return };
        let gpu = ctx.device(id).expect("device");
        let mut renderer = Renderer::new(gpu, id);
        let texture = gpu.device().create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: 8, height: 8, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        gpu.device().destroy();
        let _ = gpu.device().poll(wgpu::PollType::Poll);
        let mut targets = FrameTargets::new();
        let target = RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, &mut targets);
        let result = renderer.render(target, &Scene::new(), &FrameParams::new(peniko::Color::WHITE));
        assert!(matches!(result, Err(GpuError::DeviceLost)), "{result:?}");
    }
}
