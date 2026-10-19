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
use super::frame_targets::{FrameParams, RenderTarget};
use super::gpu_types::QuadInstance;
use super::limits::RendererLimits;
use super::prepare::{PrepareInput, PrepareScratch, VERTEX_LIMIT_NO_FULL_INDEX, prepare};
use super::stats::{FrameStats, Unsupported, WarnOnce};

/// Pipelines + caches for ONE device. Shared by every surface/target on that device.
pub struct Renderer {
    pub(crate) sample_count: u32,
    /// Resource budgets, read once (10 §8, M03).
    pub(crate) budgets: super::budgets::GpuBudgets,
    adapter: wgpu::Adapter,
    msaa: super::frame_targets::MsaaCache,
    pub(crate) shaders: ShaderModules,
    pub(crate) layouts: BindGroupLayouts,
    pub(crate) pipelines: PipelineCache,
    globals_buffer: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    /// SDF clip records (09 §2.4, L04): one `ClipParams` per slot; `clip_slots` slots.
    clip_buffer: wgpu::Buffer,
    clip_slots: u64,
    /// Dynamic-offset slots in `globals_buffer` (slot 0 = main pass, then one per Tier C run).
    globals_slots: u64,
    /// One `CompositeParams` slot per Tier C run (R17).
    composite_buffer: wgpu::Buffer,
    composite_slots: u64,
    /// This frame's `Globals::color_flags`.
    color_flags: u32,
    pub(crate) runs: Vec<super::execute::tier_c::Run>,
    pub(crate) arenas: FrameArenas,
    device: wgpu::Device,
    queue: wgpu::Queue,
    health: std::sync::Arc<super::context::DeviceHealth>,
    pub(crate) glyph_atlas: GlyphAtlas,
    pub(crate) glyph_instances: Vec<QuadInstance>,
    pub(crate) image_cache: ImageCache,
    /// Samplers by key (10 §3.5, M12).
    pub(crate) samplers: super::execute::resources::bind_groups::SamplerCache,
    pub(crate) image_instances: Vec<QuadInstance>,
    pub(crate) scratch: PrepareScratch,
    stats: FrameStats,
    /// True inside an R39 pre-pass render (no per-frame atlas/image bookkeeping, no pre-pass).
    nested: bool,
    /// Reused R39 job list.
    colr_jobs: Vec<super::prepare::ColrAtlasJob>,
    warn_once: WarnOnce,
    vertex_limit: u32,
    limits: RendererLimits,
}

impl Renderer {
    /// Debug-only, count-only cache sizes of this device (10 §9.2, M05). O(1): `len()` reads
    /// and running totals. The per-target fields stay 0 (see [`GpuCensus`](crate::GpuCensus)).
    #[cfg(debug_assertions)]
    pub fn census(&self) -> crate::census::GpuCensus {
        let mut c = crate::census::GpuCensus::default();
        self.glyph_atlas.census_into(&mut c);
        self.image_cache.census_into(&mut c);
        c.samplers = self.samplers.len();
        c.color_glyph_fragments = self.scratch.colr.len();
        c.pipelines = self.pipelines.len();
        c.arena_capacity_bytes = self.arenas.capacity_bytes();
        c
    }

    /// The MSAA sample count negotiated for the last target format (02 §5.4; 1 or 4).
    pub fn msaa_sample_count(&self) -> u32 {
        self.sample_count
    }

    /// Build the eager resource set (Globals, arenas, layouts, shader modules, samplers).
    /// The pre-C09 glyph system and image cache stay lazy.
    pub fn new(gpu: &GpuDevice, id: DeviceId) -> Self {
        let budgets = super::budgets::GpuBudgets::from_env(&gpu.device().limits());
        Self::with_budgets(gpu, id, budgets)
    }

    /// `new` with explicit budgets (tests; fields set here are never clamped, 10 §8 M03).
    #[doc(hidden)]
    pub fn with_budgets(gpu: &GpuDevice, _id: DeviceId, budgets: super::budgets::GpuBudgets) -> Self {
        let device = gpu.device();
        let layouts = BindGroupLayouts::new(device);
        let glyph_atlas = GlyphAtlas::new(device, &budgets);
        let globals_buffer = uniform_buffer(device, "pebbles.gpu.buffer.globals", 1);
        let clip_buffer = uniform_buffer(device, "pebbles.gpu.buffer.clips", 1);
        let globals_bg = globals_bind_group(device, &layouts, &globals_buffer, &clip_buffer);
        let composite_buffer = uniform_buffer(device, "pebbles.gpu.buffer.composite", 1);
        Renderer {
            sample_count: gpu.caps().msaa_samples,
            budgets,
            adapter: gpu.adapter().clone(),
            msaa: super::frame_targets::MsaaCache::default(),
            shaders: ShaderModules::new(device),
            layouts,
            pipelines: PipelineCache::new(),
            globals_buffer,
            globals_bg,
            clip_buffer,
            clip_slots: 1,
            globals_slots: 1,
            composite_buffer,
            composite_slots: 1,
            color_flags: 0,
            runs: Vec::new(),
            arenas: FrameArenas::new(),
            device: device.clone(),
            queue: gpu.queue().clone(),
            health: std::sync::Arc::clone(gpu.health()),
            glyph_atlas,
            glyph_instances: Vec::new(),
            image_cache: ImageCache::new(budgets.image_cache_bytes),
            samplers: super::execute::resources::bind_groups::SamplerCache::default(),
            image_instances: Vec::new(),
            limits: RendererLimits::from_device_limits(&device.limits()),
            scratch: PrepareScratch::default(),
            stats: FrameStats::default(),
            nested: false,
            colr_jobs: Vec::new(),
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
    pub fn last_frame_stats(&self) -> FrameStats {
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
        let nested = self.nested;
        if !nested {
            self.glyph_atlas.begin_frame();
            self.image_cache.begin_frame();
            // R39: COLR glyphs missing from the color atlas are rendered into it first.
            self.render_colr_misses(scene, params, &mut stats)?;
        }
        // The main pass is single-sampled; the negotiated count is for Tier C runs (R12, R17).
        self.sample_count = self.msaa.get(&self.adapter, format, None);

        let input = PrepareInput {
            target_size: size,
            root_transform: params.root_transform,
            vertex_limit: self.vertex_limit,
            scale_factor: params.scale_factor,
            glyph_hint: params.glyph_hint,
        };
        let started = web_time::Instant::now();
        prepare(scene, &input, &self.limits, &mut self.scratch, &mut stats);
        stats.prepare_us = u32::try_from(started.elapsed().as_micros()).unwrap_or(u32::MAX);
        stats.ops = self.scratch.plan.stats.ops;
        stats.cmds = u32::try_from(self.scratch.plan.cmds.len()).unwrap_or(u32::MAX);
        stats.culled = self.scratch.plan.stats.culled;
        {
            let plan = &self.scratch.plan;
            let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
            stats.vertices = n(plan.solid_vertices.len());
            stats.indices = n(plan.solid_indices.len());
            stats.instances =
                n(plan.shapes.len() + plan.shadows.len() + plan.glyphs.len() + plan.images.len());
            stats.segments = plan.stats.tess_calls;
        }
        let upload_started = web_time::Instant::now();

        let globals = Globals {
            target_size: [size.0 as f32, size.1 as f32],
            target_origin: [0.0, 0.0],
            inv_target_size: [1.0 / size.0 as f32, 1.0 / size.1 as f32],
            color_flags: if target.decode_srgb_output {
                super::execute::resources::globals::COLOR_FLAG_DECODE_SRGB_OUTPUT
            } else {
                0
            },
            _pad0: 0,
        };
        queue.write_buffer(&self.globals_buffer, 0, bytemuck::bytes_of(&globals));
        self.write_clips(device, queue, &mut stats);

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

        self.pipelines.prewarm(device, &self.shaders, &self.layouts, format, 1, &mut stats);
        resolve_and_upload(self, device, queue, &mut stats);
        stats.upload_us = u32::try_from(upload_started.elapsed().as_micros()).unwrap_or(u32::MAX);
        let encode_started = web_time::Instant::now();

        self.color_flags = globals.color_flags;
        let mut clear = super::prepare::color::clear_color(params.base_color);
        if target.decode_srgb_output {
            clear = super::prepare::color::linearize_clear(clear);
        }
        let mut encoder = device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("pebbles.gpu.encoder") });
        encoder.push_debug_group("pebbles.gpu.frame");
        let composites = super::execute::tier_c::encode_runs(
            self,
            device,
            queue,
            &mut encoder,
            target.targets,
            format,
            &mut stats,
        );
        super::execute::encode::ensure_plan_pipelines(
            self,
            device,
            format,
            !composites.is_empty(),
            &mut stats,
        );
        {
            let (view, resolve_target) = (target.view, None);
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
            encode(self, &mut pass, format, &mut stats, &composites);
            stats.render_passes += 1;
        }
        encoder.pop_debug_group();
        let command_buffer = encoder.finish();
        stats.encode_us = u32::try_from(encode_started.elapsed().as_micros()).unwrap_or(u32::MAX);
        let submit_started = web_time::Instant::now();
        queue.submit([command_buffer]);
        stats.submit_us = u32::try_from(submit_started.elapsed().as_micros()).unwrap_or(u32::MAX);
        target.targets.pool.end_frame(self.budgets.layer_pool_bytes);
        stats.layer_pool_bytes = target.targets.pool.bytes();
        self.scratch.plan.release_refs();
        if !nested {
            self.glyph_atlas.end_frame(&mut stats);
            self.image_cache.end_frame(&mut stats);
        }
        self.end_frame(&stats);
        self.stats = stats;
        Ok(stats)
    }
}

impl Renderer {
    /// Write this frame's SDF clip records (09 §2.4, L04), growing the clip buffer (and so the
    /// group-0 bind group) when needed. Record 0 is `ClipParams::NONE`.
    fn write_clips(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, stats: &mut FrameStats) {
        let n = self.scratch.plan.sdf_clips.len().max(1) as u64;
        if n > self.clip_slots {
            let slots = n.next_power_of_two();
            self.clip_buffer = uniform_buffer(device, "pebbles.gpu.buffer.clips", slots);
            self.clip_slots = slots;
            self.globals_bg =
                globals_bind_group(device, &self.layouts, &self.globals_buffer, &self.clip_buffer);
            stats.buffers_created += 1;
            stats.bind_groups_created += 1;
        }
        let stride = super::execute::tier_c::slot_stride(device);
        for (i, c) in self.scratch.plan.sdf_clips.iter().enumerate() {
            let p = super::gpu_types::ClipParams { rect: c.rect, radii: c.radii };
            queue.write_buffer(&self.clip_buffer, i as u64 * stride, bytemuck::bytes_of(&p));
        }
    }

    /// The dynamic offset of SDF clip record `id` (09 §2.4).
    pub(crate) fn clip_offset(&self, id: super::prepare::SdfClipId) -> u32 {
        let stride = super::execute::tier_c::slot_stride(&self.device);
        u32::try_from(u64::from(id.0) * stride).unwrap_or(0)
    }

    /// R39 pre-pass: render every atlas-route COLR glyph of `scene` that misses the color atlas
    /// into its slot — once, with the normal pipelines, into a scratch `Rgba8Unorm` target that
    /// is copied into the page (pages are not render attachments). A frame with no misses does
    /// no extra work.
    fn render_colr_misses(
        &mut self,
        scene: &Scene,
        params: &FrameParams,
        stats: &mut FrameStats,
    ) -> Result<(), GpuError> {
        let mut jobs = std::mem::take(&mut self.colr_jobs);
        jobs.clear();
        if !self.scratch.colr_vector_only {
            super::prepare::scan_colr_atlas(
                scene.ops(),
                params.root_transform,
                &mut self.scratch.colr,
                &mut jobs,
            );
        }
        let mut todo = Vec::new();
        for (i, job) in jobs.iter().enumerate() {
            let key = job.glyph.key;
            if self.glyph_atlas.cpu.get(key).is_some() {
                continue; // a hit (now pinned for this frame)
            }
            let (k, bin, b) = (job.glyph.k, job.glyph.bin_dx, job.fragment.bounds);
            // Fragment space is font units, y-up: device = (bin + x·k, −y·k) from the pen.
            let (x0, x1) = ((bin + b.x0 * k).floor() - 1.0, (bin + b.x1 * k).ceil() + 1.0);
            let (y0, y1) = ((-b.y1 * k).floor() - 1.0, (-b.y0 * k).ceil() + 1.0);
            let to_i = |v: f64| super::prepare::num::to_i32_sat(v);
            let (Ok(w), Ok(h)) = (u32::try_from(to_i(x1 - x0)), u32::try_from(to_i(y1 - y0))) else {
                continue;
            };
            if w == 0 || h == 0 {
                continue;
            }
            if let Some(slot) = self.glyph_atlas.insert_colr(key, w, h, to_i(x0), to_i(-y0), stats) {
                todo.push((i, slot, x0, y0));
            }
        }
        if !todo.is_empty() {
            let (device, queue) = (self.device.clone(), self.queue.clone());
            self.glyph_atlas.flush(&device, &queue, &self.layouts.glyph, stats);
            for (i, slot, x0, y0) in todo {
                let Some(job) = jobs.get(i) else { continue };
                let (w, h) = (u32::from(slot.w), u32::from(slot.h));
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("pebbles.gpu.colr.scratch"),
                    size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                });
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                stats.textures_created += 1;
                stats.texture_views_created += 1;
                let m = kurbo::Affine::translate((-x0, -y0))
                    * kurbo::Affine::translate((job.glyph.bin_dx, 0.0))
                    * kurbo::Affine::scale_non_uniform(job.glyph.k, -job.glyph.k);
                let mut glyph_scene = Scene::new();
                pebbles_render::paint::Painter::new(&mut glyph_scene).append(&job.fragment.scene, Some(m));
                let mut targets = super::frame_targets::FrameTargets::new();
                let target = RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, &mut targets);
                self.nested = true;
                let rendered =
                    self.render(target, &glyph_scene, &FrameParams::new(peniko::Color::TRANSPARENT));
                self.nested = false;
                rendered?;
                let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("pebbles.gpu.encoder.colr_copy"),
                });
                enc.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: self.glyph_atlas.color_texture(),
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: u32::from(slot.x),
                            y: u32::from(slot.y),
                            z: u32::from(slot.layer),
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                );
                queue.submit([enc.finish()]);
                stats.color_glyph_renders += 1;
            }
        }
        self.colr_jobs = jobs;
        Ok(())
    }

    /// Debug switch (R39 tests): draw atlas-route COLR glyphs as vector fragments instead.
    #[doc(hidden)]
    pub fn set_colr_atlas_enabled(&mut self, enabled: bool) {
        self.scratch.colr_vector_only = !enabled;
    }

    /// After the last submit: log each fallback kind the first time it occurs (06 §6.3).
    fn end_frame(&mut self, stats: &FrameStats) {
        for kind in Unsupported::ALL {
            self.warn_once.report(kind, stats.unsupported[kind as usize]);
        }
    }

    pub(crate) fn globals_bg(&self) -> &wgpu::BindGroup {
        &self.globals_bg
    }

    /// `Globals::color_flags` of the current frame (slot 0), reused by Tier C run slots.
    pub(crate) fn color_flags(&self) -> u32 {
        self.color_flags
    }

    pub(crate) fn globals_buffer(&self) -> &wgpu::Buffer {
        &self.globals_buffer
    }

    pub(crate) fn composite_buffer(&self) -> &wgpu::Buffer {
        &self.composite_buffer
    }

    /// Generation of the composite params buffer (its slot count only grows).
    pub(crate) fn composite_generation(&self) -> u64 {
        self.composite_slots
    }

    /// Grow the globals / composite uniform buffers to hold at least the given slot counts.
    pub(crate) fn ensure_uniform_slots(
        &mut self,
        device: &wgpu::Device,
        globals: u64,
        composites: u64,
        stats: &mut FrameStats,
    ) {
        if globals > self.globals_slots {
            let n = globals.next_power_of_two();
            let buffer = uniform_buffer(device, "pebbles.gpu.buffer.globals", n);
            let bg = globals_bind_group(device, &self.layouts, &buffer, &self.clip_buffer);
            // Slot 0 (the main pass) was written this frame already: copy it over.
            let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("pebbles.gpu.encoder.globals_grow"),
            });
            enc.copy_buffer_to_buffer(&self.globals_buffer, 0, &buffer, 0, size_of::<Globals>() as u64);
            self.queue.submit([enc.finish()]);
            (self.globals_buffer, self.globals_bg, self.globals_slots) = (buffer, bg, n);
            stats.buffers_created += 1;
            stats.bind_groups_created += 1;
        }
        if composites > self.composite_slots {
            let n = composites.next_power_of_two();
            self.composite_buffer = uniform_buffer(device, "pebbles.gpu.buffer.composite", n);
            self.composite_slots = n;
            stats.buffers_created += 1;
        }
    }
}

/// A uniform buffer of `slots` dynamic-offset slots.
fn uniform_buffer(device: &wgpu::Device, label: &'static str, slots: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: slots * super::execute::tier_c::slot_stride(device),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    })
}

/// Group 0 over the Globals (binding 0) and SDF clip (binding 1) buffers, both 32-byte windows
/// with dynamic offsets (09 §2.4).
fn globals_bind_group(
    device: &wgpu::Device,
    layouts: &BindGroupLayouts,
    globals: &wgpu::Buffer,
    clips: &wgpu::Buffer,
) -> wgpu::BindGroup {
    let window = |buffer| {
        wgpu::BindingResource::Buffer(wgpu::BufferBinding {
            buffer,
            offset: 0,
            size: wgpu::BufferSize::new(32),
        })
    };
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("pebbles.gpu.bind.globals"),
        layout: &layouts.globals,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: window(globals) },
            wgpu::BindGroupEntry { binding: 1, resource: window(clips) },
        ],
    })
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
        let (_texture, view) = crate::test_support::target_8x8(gpu.device());
        gpu.device().destroy();
        let _ = gpu.device().poll(wgpu::PollType::Poll);
        let mut targets = FrameTargets::new();
        let target = RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, &mut targets);
        let result = renderer.render(target, &Scene::new(), &FrameParams::new(peniko::Color::WHITE));
        assert!(matches!(result, Err(GpuError::DeviceLost)), "{result:?}");
    }
}
