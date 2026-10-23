//! One [`Renderer`] per device: the eager resource set (Globals, arenas, layouts, pipeline
//! cache, glyph atlas).

use pebbles_render::paint::Scene;

use super::context::{DeviceId, GpuDevice};
use super::error::GpuError;
use super::execute::encode::{encode, resolve_frame};
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
use super::prepare::damage::{Damage, FrameKey};
use super::prepare::{PrepareInput, PrepareScratch, VERTEX_LIMIT_NO_FULL_INDEX, prepare};
use super::stats::{FrameStats, Unsupported, WarnOnce};

// 08 §17.1 T3/T4: the render thread owns these (native only; wgpu handles are `!Send` on wasm).
#[cfg(not(target_family = "wasm"))]
const _: () = {
    const fn assert_send<T: Send>() {}
    assert_send::<crate::GpuContext>();
    assert_send::<Renderer>();
    assert_send::<crate::RenderSurface<'static>>();
};

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
    /// This frame's Globals slots, uploaded once (10 §2.6, M08).
    pub(crate) globals_staging: super::execute::resources::globals::UniformSlots,
    globals_bg: wgpu::BindGroup,
    /// SDF clip records (09 §2.4, L04): one `ClipParams` per slot; `clip_slots` slots.
    clip_buffer: wgpu::Buffer,
    clip_slots: u64,
    /// Mask-clip records (09 L08) and the inputs group 0 was last built from.
    mask_clip_buffer: wgpu::Buffer,
    mask_clip_slots: u64,
    globals_bg_key: (u64, u64, u64, u64),
    mask_clip_slots_resolved: Vec<Option<super::execute::resources::atlas_alloc::AtlasSlot>>,
    /// Dynamic-offset slots in `globals_buffer` (slot 0 = main pass, then one per Tier C run).
    globals_slots: u64,
    /// One `CompositeParams` slot per Tier C run (R17).
    composite_buffer: wgpu::Buffer,
    composite_slots: u64,
    /// This frame's `Globals::color_flags`.
    color_flags: u32,
    pub(crate) runs: Vec<super::execute::tier_c::Run>,
    /// Image-brush params and bind groups (09 §6, L42).
    pub(crate) image_brushes: super::execute::image_brushes::ImageBrushResources,
    /// Effect-pass params (09 §10): one dynamic-offset slot per pass of the frame.
    effect_buffer: wgpu::Buffer,
    effect_slots: u64,
    /// 1×1 white texture + sampler with the image layout (R63).
    white_bg: wgpu::BindGroup,
    _white_tex: wgpu::Texture,
    /// Projected-layer params (09 §10.7), filled in frame order.
    proj_buffer: wgpu::Buffer,
    proj_slots: u64,
    proj_next: u32,
    /// Gradient LUT and params (09 §5.3, L34/L35).
    pub(crate) gradients: super::execute::gradients::GradientResources,
    /// GPU coverage masks: the mask pool and the per-texture group-0 variants (09 §2.9, L09).
    pub(crate) gpu_masks: super::execute::gpu_masks::GpuMasks,
    /// `layer_budget_exceeded` already warned (once per renderer).
    layer_budget_warned: bool,
    /// Rebased index / joined image scratch for later segments (M07).
    seg_indices: Vec<u32>,
    seg_quads: Vec<QuadInstance>,
    /// Array bases of the segment being encoded (M07; zero for single-segment frames).
    pub(crate) seg_base: super::prepare::segments::SegBase,
    /// Custom renderers by paint type (02 §11, R60).
    customs: super::execute::custom::CustomRegistry,
    /// Shader-brush programs and per-frame bindings (09 §6.1, L43).
    pub(crate) shader_programs: super::execute::resources::shader_programs::ShaderPrograms,
    pub(crate) shader_brushes: super::execute::shader_brushes::ShaderBrushResources,
    /// The targets this Renderer renders, by id (10 §4.4, M13).
    targets_book: super::frame_targets::TargetBook,
    /// Frames rendered (the book's clock).
    frame: u64,
    /// `Renderer::new` instant until "renderer ready" is logged (10 §12.1, M23).
    created_at: Option<web_time::Instant>,
    /// Opt-in GPU frame timing (M22); `None` without `TIMESTAMP_QUERY`.
    timer: Option<super::timing::GpuTimer>,
    /// Pass segments, reused across frames (09 §3.6).
    pub(crate) segments: Vec<super::prepare::passes::PassSegment>,
    pub(crate) arenas: FrameArenas,
    device: wgpu::Device,
    queue: wgpu::Queue,
    health: std::sync::Arc<super::context::DeviceHealth>,
    pub(crate) glyph_atlas: GlyphAtlas,
    pub(crate) glyph_instances: Vec<QuadInstance>,
    pub(crate) image_cache: ImageCache,
    /// App-owned textures for `Op::GpuImage` (02 §11.1, R61).
    pub(crate) external_textures: super::execute::resources::image_cache::ExternalTextures,
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
        (c.gpu_mask_textures, c.gpu_mask_bytes) = self.gpu_masks.census();
        let (_, rows_used, rows_total) = self.gradients.lut.census();
        (c.gradient_rows_used, c.gradient_rows_capacity) = (rows_used as usize, rows_total);
        c.arena_capacity_bytes = self.arenas.capacity_bytes();
        c.retained_entries = self.scratch.retained.len();
        c.retained_bytes = self.scratch.retained.bytes();
        c.frame_target_entries = self.targets_book.len();
        c.frame_target_bytes = self.targets_book.total_bytes();
        c
    }

    /// Upload segment `k`'s slices of every array into the arenas and set `seg_base` (10 §2.5,
    /// M07). Indices are rebased to the segment's first vertex; a single-segment frame uploads
    /// the arrays as they are. False when an arena could not grow (11 §2.6).
    fn upload_segment(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        k: usize,
        stats: &mut FrameStats,
    ) -> bool {
        use super::prepare::segments::{
            GLYPHS, GPU_IMAGES, IMAGES, INDICES, MESHES, SHADOWS, SHAPES, SegBase, VERTICES,
        };
        let Some(seg) = self.scratch.plan.segments.get(k) else {
            self.seg_base = SegBase::default();
            return true;
        };
        self.seg_base = SegBase::of(seg);
        let a = seg.arrays.clone();
        let r = |x: &std::ops::Range<u32>| x.start as usize..x.end as usize;
        let plan = &self.scratch.plan;
        let arenas = &mut self.arenas;
        let mut ok = arenas.solid_vertices.upload(
            device,
            queue,
            bytemuck::cast_slice(plan.solid_vertices.get(r(&a[VERTICES])).unwrap_or_default()),
            stats,
        );
        let indices = plan.solid_indices.get(r(&a[INDICES])).unwrap_or_default();
        let vbase = a[VERTICES].start;
        let indices: &[u32] = if vbase == 0 {
            indices
        } else {
            self.seg_indices.clear();
            self.seg_indices.extend(indices.iter().map(|i| i.saturating_sub(vbase)));
            &self.seg_indices
        };
        ok &= arenas.solid_indices.upload(device, queue, bytemuck::cast_slice(indices), stats);
        ok &= arenas.shape_instances.upload(
            device,
            queue,
            bytemuck::cast_slice(plan.shapes.get(r(&a[SHAPES])).unwrap_or_default()),
            stats,
        );
        ok &= arenas.shadow_instances.upload(
            device,
            queue,
            bytemuck::cast_slice(plan.shadows.get(r(&a[SHADOWS])).unwrap_or_default()),
            stats,
        );
        ok &= arenas.glyphs.upload(
            device,
            queue,
            bytemuck::cast_slice(self.glyph_instances.get(r(&a[GLYPHS])).unwrap_or_default()),
            stats,
        );
        // Images then this segment's GPU-image instances (stored after every image).
        let n_img = u32::try_from(plan.images.len()).unwrap_or(u32::MAX);
        let (img, gpu) =
            (r(&a[IMAGES]), (n_img + a[GPU_IMAGES].start) as usize..(n_img + a[GPU_IMAGES].end) as usize);
        let images: &[QuadInstance] = if img.end == gpu.start || gpu.is_empty() {
            self.image_instances.get(img.start..gpu.end.max(img.end)).unwrap_or_default()
        } else {
            self.seg_quads.clear();
            self.seg_quads.extend_from_slice(self.image_instances.get(img).unwrap_or_default());
            self.seg_quads.extend_from_slice(self.image_instances.get(gpu).unwrap_or_default());
            &self.seg_quads
        };
        ok &= arenas.images.upload(device, queue, bytemuck::cast_slice(images), stats);
        ok &= arenas.meshes.upload(
            device,
            queue,
            bytemuck::cast_slice(plan.mesh_vertices.get(r(&a[MESHES])).unwrap_or_default()),
            stats,
        );
        ok
    }

    /// Render segment `k`'s GPU mask jobs and Tier C runs into their booked targets (M07).
    fn render_segment_offscreen(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        k: usize,
        targets: &super::frame_targets::FrameTargets,
        composites: &[super::execute::tier_c::Composite],
        format: wgpu::TextureFormat,
        stats: &mut FrameStats,
    ) {
        let Some(seg) = self.scratch.plan.segments.get(k).cloned() else { return };
        super::execute::gpu_masks::render_jobs(self, encoder, seg.jobs.clone(), stats);
        super::execute::tier_c::render_runs(
            self,
            encoder,
            targets,
            composites,
            seg.cmds.clone(),
            format,
            stats,
        );
    }

    /// The bytes of GPU memory this Renderer requested (10 §9.3, M17). On demand only: the
    /// driver report allocates, so never call it per frame. `layer_pool_bytes` is the last
    /// rendered target's pool.
    pub fn memory_report(&self) -> crate::MemoryReport {
        let (glyph_mask_bytes, glyph_color_bytes) = self.glyph_atlas.bytes();
        let stride = super::execute::tier_c::slot_stride(&self.device);
        let uniforms = (self.globals_slots + self.composite_slots) * stride;
        crate::MemoryReport {
            frame_targets_bytes: self.targets_book.total_bytes(),
            glyph_mask_bytes,
            glyph_color_bytes,
            image_cache_bytes: self.image_cache.bytes(),
            gradient_lut_bytes: self.gradients.lut.bytes(),
            layer_pool_bytes: self.stats.layer_pool_bytes,
            gpu_mask_bytes: self.gpu_masks.bytes(),
            arena_bytes: self.arenas.capacity_bytes() + uniforms,
            driver_allocated_bytes: self.device.generate_allocator_report().map(|r| r.total_allocated_bytes),
        }
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
        let mask_clip_buffer = uniform_buffer(device, "pebbles.gpu.buffer.mask_clips", 1);
        let (mask_view, mask_gen) = glyph_atlas.mask_view();
        let gpu_masks = super::execute::gpu_masks::GpuMasks::new(device);
        let lut_rows = budgets.gradient_lut_max_rows;
        let (white_tex, white_bg) = white_texture(device, gpu.queue(), &layouts);
        let globals_bg = globals_bind_group(
            device,
            &layouts,
            &globals_buffer,
            &clip_buffer,
            &mask_clip_buffer,
            mask_view,
            gpu_masks.dummy_view(),
        );
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
            globals_staging: Default::default(),
            globals_bg,
            clip_buffer,
            clip_slots: 1,
            mask_clip_buffer,
            mask_clip_slots: 1,
            globals_bg_key: (1, 1, 1, mask_gen),
            mask_clip_slots_resolved: Vec::new(),
            globals_slots: 1,
            composite_buffer,
            composite_slots: 1,
            color_flags: 0,
            runs: Vec::new(),
            gpu_masks,
            gradients: super::execute::gradients::GradientResources::new(device, lut_rows),
            image_brushes: super::execute::image_brushes::ImageBrushResources::new(device),
            effect_buffer: uniform_buffer(device, "pebbles.gpu.buffer.effects", 1),
            effect_slots: 1,
            white_bg,
            _white_tex: white_tex,
            proj_buffer: uniform_buffer(device, "pebbles.gpu.buffer.projected", 4),
            proj_slots: 4,
            proj_next: 0,
            segments: Vec::new(),
            customs: Default::default(),
            seg_base: Default::default(),
            seg_indices: Vec::new(),
            seg_quads: Vec::new(),
            timer: super::timing::GpuTimer::new(device, gpu.queue()),
            created_at: Some(web_time::Instant::now()),
            targets_book: super::frame_targets::TargetBook::new(budgets.frame_targets_bytes),
            frame: 0,
            shader_programs: Default::default(),
            shader_brushes: super::execute::shader_brushes::ShaderBrushResources::new(
                gpu.device(),
                gpu.queue(),
            ),
            external_textures: Default::default(),
            layer_budget_warned: false,
            arenas: FrameArenas::new(gpu.device().limits().max_buffer_size.min(budgets.arena_max_bytes)),
            device: device.clone(),
            queue: gpu.queue().clone(),
            health: std::sync::Arc::clone(gpu.health()),
            glyph_atlas,
            glyph_instances: Vec::new(),
            image_cache: ImageCache::new(budgets.image_cache_bytes),
            samplers: super::execute::resources::bind_groups::SamplerCache::default(),
            image_instances: Vec::new(),
            limits: RendererLimits::from_device_limits(&device.limits()),
            scratch: PrepareScratch {
                gpu_mask_budget: Some(budgets.gpu_mask_bytes),
                path_mask_budget: Some(budgets.path_mask_bytes),
                retained_budget: budgets.retained_bytes,
                // M07: segments fit the arenas' caps and the portable vertex rule.
                arena_caps: super::prepare::segments::ArenaCaps {
                    bytes: gpu.device().limits().max_buffer_size.min(budgets.arena_max_bytes) / 4 * 4,
                    max_vertices: super::prepare::segments::MAX_PORTABLE_VERTICES_PER_SEGMENT,
                },
                ..PrepareScratch::default()
            },
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

    /// Draw every `Op::Custom` whose paint is a `P` with `r` (02 §11.2, R60); replaces an earlier
    /// registration for `P`.
    pub fn register_custom<P: super::CustomPaint>(&mut self, r: Box<dyn super::CustomRenderer>) {
        self.customs.insert(std::any::TypeId::of::<P>(), r, None);
    }

    /// [`register_custom`](Self::register_custom) with a depth attachment of `depth` (e.g.
    /// `Depth32Float`, 02 §11.1, R62): `CustomRender::depth` is then a texture matching the
    /// target, created on first use and cleared to 1.0 before every custom pass.
    pub fn register_custom_with_depth<P: super::CustomPaint>(
        &mut self,
        r: Box<dyn super::CustomRenderer>,
        depth: wgpu::TextureFormat,
    ) {
        self.customs.insert(std::any::TypeId::of::<P>(), r, Some(depth));
    }

    /// Make an app-owned texture drawable with `Op::GpuImage` (02 §11.1, R61). The view must be
    /// a filterable float 2D texture of this device holding premultiplied, sRGB-encoded texels
    /// (like uploaded images, 02 §3). It is never copied and never evicted: call
    /// [`unregister_texture`](Self::unregister_texture) when done. Ids are never reused.
    pub fn register_texture(
        &mut self,
        view: wgpu::TextureView,
        size: (u32, u32),
    ) -> pebbles_render::paint::display_list::TextureId {
        pebbles_render::paint::display_list::TextureId(self.external_textures.register(view, size))
    }

    /// Forget a texture registered with [`register_texture`](Self::register_texture); later
    /// draws of `id` are skipped and counted. Returns whether `id` was registered.
    pub fn unregister_texture(&mut self, id: pebbles_render::paint::display_list::TextureId) -> bool {
        self.external_textures.unregister(id.0)
    }

    /// The size passed to [`register_texture`](Self::register_texture), if `id` is registered.
    pub fn texture_size(&self, id: pebbles_render::paint::display_list::TextureId) -> Option<(u32, u32)> {
        self.external_textures.size(id.0)
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
        stats.layers_folded = self.scratch.plan.stats.layers_folded;
        stats.layers_offscreen = self.scratch.plan.stats.layers_offscreen;
        {
            let plan = &self.scratch.plan;
            let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
            stats.vertices = n(plan.solid_vertices.len());
            stats.indices = n(plan.solid_indices.len());
            stats.instances =
                n(plan.shapes.len() + plan.shadows.len() + plan.glyphs.len() + plan.images.len());
            stats.segments = n(plan.segments.len());
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
        // 10 §2.6: every pass target's slot is staged, then uploaded once before the passes.
        self.globals_staging.begin(super::execute::tier_c::slot_stride(device));
        let root_slot = self.globals_staging.push(&globals);
        debug_assert_eq!(root_slot, 0);
        self.write_clips(device, queue, &mut stats);

        self.pipelines.prewarm(device, &self.shaders, &self.layouts, format, 1, &mut stats);
        if let Some(t) = self.created_at.take() {
            // 10 §12.1: Renderer::new through the first target's prewarm, once per renderer.
            pebbles_foundation::log::logf(
                pebbles_foundation::log::Level::Info,
                pebbles_foundation::log::Cat::Perf,
                format_args!(
                    "renderer ready in {:.1} ms ({} pipelines prewarmed)",
                    t.elapsed().as_secs_f64() * 1000.0,
                    super::execute::pipelines::PipelineKind::ALL.iter().filter(|k| k.prewarm()).count()
                ),
            );
        }
        resolve_frame(self, device, queue, &mut stats);
        // M30: a frame whose draws were dropped leaves the persistent root stale.
        let mut draws_dropped = false;
        // M07: the first segment's slices (the only one on normal frames).
        if !self.upload_segment(device, queue, 0, &mut stats) {
            draws_dropped = true;
            // 11 §2.6: an arena could not grow even after a retry — skip this frame's draws
            // (the passes still clear; nothing references the missing data).
            let plan = &mut self.scratch.plan;
            plan.cmds.clear();
            plan.gpu_mask_jobs.clear();
            plan.layers.clear();
            plan.customs.clear();
            plan.segments.clear();
            plan.root_needs_backdrop = false;
        }
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
        if !nested && let Some(t) = &mut self.timer {
            t.begin(&mut encoder, target.view);
        }
        // O3: every target and Globals slot of the frame is booked before the first segment.
        super::execute::gpu_masks::prepare_jobs(self, device, &mut stats);
        super::execute::gradients::prepare(self, device, queue, format, &mut stats);
        super::execute::image_brushes::prepare(self, device, queue, format, &mut stats);
        super::execute::shader_brushes::prepare(self, device, queue, format, &mut stats);
        let composites =
            super::execute::tier_c::prepare_runs(self, device, queue, target.targets, format, &mut stats);
        self.render_segment_offscreen(&mut encoder, 0, target.targets, &composites, format, &mut stats);
        let layers =
            super::execute::layers::acquire_layers(self, device, queue, target.targets, format, &mut stats);
        super::execute::encode::ensure_plan_pipelines(
            self,
            device,
            format,
            !composites.is_empty() || !layers.is_empty(),
            &mut stats,
        );
        // R60: custom renderers prepare before any pass; unregistered types are counted once
        // per op and skipped at execute.
        for item in &self.scratch.plan.customs {
            match self.customs.get_mut(&*item.paint) {
                Some(e) => e.renderer.prepare(&mut super::CustomPrepare { device, queue }, &*item.paint),
                None => stats.record(Unsupported::CustomUnregistered),
            }
        }
        queue.write_buffer(&self.globals_buffer, 0, self.globals_staging.bytes());
        let mut segments = std::mem::take(&mut self.segments);
        super::prepare::passes::segment_passes(&self.scratch.plan, &mut segments);
        // 09 §4.4: a backdrop composite into the root copies from it, so the root renders into a
        // copyable intermediate that is blitted to the frame target at the end. M30: a target
        // that tracks damage always does — that intermediate is its persistent frame.
        let tracking = !nested && target.targets.tracks_damage();
        let root_color = if self.scratch.plan.root_needs_backdrop || tracking {
            target
                .targets
                .root_color(device, size, format, &mut stats)
                .map(|r| (r.texture.clone(), r.view.clone()))
        } else {
            None
        };
        let damage = self.frame_damage(
            scene,
            params,
            target.targets,
            target.decode_srgb_output,
            tracking && root_color.is_some(),
            size,
        );
        // A partial frame renders its damage into a cleared scratch target, copied into the root.
        let scratch = match damage {
            Damage::Rect(_) => target
                .targets
                .damage_scratch(device, size, format, &mut stats)
                .map(|r| (r.texture.clone(), r.view.clone())),
            Damage::Full | Damage::Empty => None,
        };
        let damage =
            if matches!(damage, Damage::Rect(_)) && scratch.is_none() { Damage::Full } else { damage };
        match damage {
            Damage::Full => {
                stats.full_redraws += 1;
                stats.damage_px = u64::from(size.0) * u64::from(size.1);
            }
            Damage::Rect(r) => stats.damage_px = u64::from(r.x1 - r.x0) * u64::from(r.y1 - r.y0),
            Damage::Empty => stats.damage_px = 0,
        }
        let damage_rect = match damage {
            Damage::Rect(r) => Some(r),
            Damage::Full | Damage::Empty => None,
        };
        target.targets.end_frame_root(root_color.is_some());
        let root_view = match (&scratch, &root_color) {
            (Some(s), _) => &s.1,
            (None, Some(r)) => &r.1,
            (None, None) => target.view,
        };
        // M30: nothing changed — the persistent root is presented as it is.
        for seg in segments.iter().filter(|_| damage != Damage::Empty) {
            let Some(seg_target) = super::execute::layers::segment_target(seg.target, size, &layers) else {
                continue;
            };
            let (view, resolve_target, clear_to, label) = match seg.target {
                super::prepare::plan::TargetRef::Root => (root_view, None, clear, "pebbles.gpu.pass.root"),
                super::prepare::plan::TargetRef::Layer(i) => {
                    let Some(l) = layers[i as usize].as_ref() else { continue };
                    let tex = target.targets.pool.textures(l.slot);
                    // M26: a multisampled layer resolves only where the plan says so (its final
                    // pass, or before a backdrop capture of it).
                    let (v, r) = match &tex.msaa {
                        Some((_, m)) => (m, seg.resolve.then_some(&tex.color.1)),
                        None => (&tex.color.1, None),
                    };
                    (v, r, wgpu::Color::TRANSPARENT, "pebbles.gpu.pass.layer")
                }
            };
            let load = match seg.color_load {
                super::prepare::passes::SegmentLoad::Clear => wgpu::LoadOp::Clear(clear_to),
                super::prepare::passes::SegmentLoad::Load => wgpu::LoadOp::Load,
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(label),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target,
                    // M26 (10 §4.6): after the target's final resolve nothing loads its MSAA
                    // color again — discard it (free on tilers). Every other pass stores.
                    ops: wgpu::Operations {
                        load,
                        store: if resolve_target.is_some() && seg.is_final {
                            wgpu::StoreOp::Discard
                        } else {
                            wgpu::StoreOp::Store
                        },
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let range = seg.cmds.start as usize..seg.cmds.end as usize;
            // 09 L52: layer segments are grouped for frame debuggers.
            let layer_seg = matches!(seg.target, super::prepare::plan::TargetRef::Layer(_));
            if layer_seg {
                pass.push_debug_group("pebbles.gpu.layer");
            }
            let seg_damage = match seg.target {
                super::prepare::plan::TargetRef::Root => damage_rect,
                // Layers render whole (their targets are cleared every frame).
                super::prepare::plan::TargetRef::Layer(_) => None,
            };
            encode(self, &mut pass, format, &mut stats, &composites, range, seg_target, &layers, seg_damage);
            if layer_seg {
                pass.pop_debug_group();
            }
            drop(pass);
            stats.render_passes += 1;
            if let super::prepare::plan::TargetRef::Layer(i) = seg.target
                && seg.is_final
                && let Some(l) = layers.get(i as usize).and_then(Option::as_ref)
                && !l.effect_passes.is_empty()
            {
                self.run_effect(device, &mut encoder, l, target.targets, format, &mut stats);
            }
            if let super::prepare::passes::AfterPass::Custom(i) = seg.after
                && let Some(&super::prepare::plan::DrawCmd::Custom { op }) =
                    self.scratch.plan.cmds.get(i as usize)
                && let Some(item) = self.scratch.plan.customs.get(op as usize)
                && let Some(entry) = self.customs.get_mut(&*item.paint)
            {
                let layer_slot = match seg.target {
                    super::prepare::plan::TargetRef::Root => None,
                    super::prepare::plan::TargetRef::Layer(l) => {
                        let Some(l) = layers.get(l as usize).and_then(Option::as_ref) else { continue };
                        Some(l.slot)
                    }
                };
                // The attachment's real size and sample count (pool slots may be larger than
                // the layer).
                let (tex_size, samples) = match layer_slot {
                    None => (size, 1),
                    Some(slot) => {
                        let tex = target.targets.pool.textures(slot);
                        let t = &tex.color.0;
                        ((t.width(), t.height()), tex.msaa.as_ref().map_or(1, |(m, _)| m.sample_count()))
                    }
                };
                let (ox, oy) = seg_target.origin;
                let (w, h) = seg_target.size;
                let s = item.scissor;
                let (x0, y0) = (s.x0.saturating_sub(ox).min(w), s.y0.saturating_sub(oy).min(h));
                let (x1, y1) = (s.x1.saturating_sub(ox).min(w), s.y1.saturating_sub(oy).min(h));
                if x1 > x0 && y1 > y0 {
                    // R62: a depth-requesting renderer gets a lazily created, freshly cleared
                    // depth texture matching the attachment.
                    let depth_key = entry.depth.map(|f| {
                        let key = super::frame_targets::TargetKey {
                            width: tex_size.0,
                            height: tex_size.1,
                            format,
                            sample_count: samples,
                        };
                        (key, f)
                    });
                    if let Some((key, f)) = depth_key {
                        target.targets.ensure_depth(device, key, f, &mut stats);
                        if let Some(d) = target.targets.depth_view(key, f) {
                            clear_depth(&mut encoder, d);
                            stats.render_passes += 1;
                        }
                    }
                    let view = match layer_slot {
                        None => root_view,
                        Some(slot) => {
                            let tex = target.targets.pool.textures(slot);
                            tex.msaa.as_ref().map_or(&tex.color.1, |(_, m)| m)
                        }
                    };
                    encoder.push_debug_group("pebbles.gpu.custom");
                    let mut ctx = super::CustomRender {
                        device,
                        queue,
                        encoder: &mut encoder,
                        target: view,
                        format,
                        sample_count: samples,
                        size: tex_size,
                        scissor: (x0, y0, x1 - x0, y1 - y0),
                        transform: kurbo::Affine::translate((-f64::from(ox), -f64::from(oy)))
                            * item.transform,
                        depth: depth_key.and_then(|(k, f)| target.targets.depth_view(k, f)),
                    };
                    entry.renderer.render(&mut ctx, &*item.paint);
                    encoder.pop_debug_group();
                }
            }
            if let super::prepare::passes::AfterPass::CopyBackdrop(l) = seg.after {
                let root = root_color.as_ref().map(|r| &r.0);
                self.copy_backdrop(&mut encoder, l, &layers, target.targets, root, &mut stats);
            }
            if let super::prepare::passes::AfterPass::NextSegment(k) = seg.after {
                // M07: submit this segment, upload the next one's slices (the arenas are reused,
                // so a submit must separate them), and render its offscreen jobs and runs.
                encoder.pop_debug_group();
                let next = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("pebbles.gpu.encoder.segment"),
                });
                queue.submit([std::mem::replace(&mut encoder, next).finish()]);
                encoder.push_debug_group("pebbles.gpu.frame");
                if !self.upload_segment(device, queue, k as usize, &mut stats) {
                    // 11 §2.6: the rest of the frame's draws are skipped.
                    draws_dropped = true;
                    break;
                }
                self.render_segment_offscreen(
                    &mut encoder,
                    k as usize,
                    target.targets,
                    &composites,
                    format,
                    &mut stats,
                );
            }
        }
        self.segments = segments;
        // M30: the redrawn damage joins the persistent root.
        if let (Some(r), Some((scratch, _)), Some((root, _))) = (damage_rect, &scratch, &root_color) {
            let at = |texture| wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: r.x0, y: r.y0, z: 0 },
                aspect: wgpu::TextureAspect::All,
            };
            encoder.copy_texture_to_texture(
                at(scratch),
                at(root),
                wgpu::Extent3d { width: r.x1 - r.x0, height: r.y1 - r.y0, depth_or_array_layers: 1 },
            );
        }
        if draws_dropped {
            target.targets.reset_damage();
        }
        if root_color.is_some()
            && let Some(group) = target.targets.root_blit_group(device, &self.layouts.blit, &mut stats)
        {
            self.blit_root(device, &mut encoder, &group, target.view, format, &mut stats);
        }
        if !nested && let Some(t) = &mut self.timer {
            t.end(&mut encoder, target.view);
        }
        encoder.pop_debug_group();
        let command_buffer = encoder.finish();
        stats.encode_us = u32::try_from(encode_started.elapsed().as_micros()).unwrap_or(u32::MAX);
        let submit_started = web_time::Instant::now();
        queue.submit([command_buffer]);
        if !nested && let Some(t) = &mut self.timer {
            t.after_submit();
            stats.gpu_us = t.poll(device);
        }
        if !nested {
            self.arenas.end_frame(device, &mut stats);
        }
        stats.submit_us = u32::try_from(submit_started.elapsed().as_micros()).unwrap_or(u32::MAX);
        if target.targets.pool.end_frame(self.budgets.layer_pool_bytes) {
            // 09 §3.8: in-use targets are never refused; count it and warn once.
            stats.layer_budget_exceeded += 1;
            if !self.layer_budget_warned {
                self.layer_budget_warned = true;
                pebbles_foundation::log::logf(
                    pebbles_foundation::log::Level::Warn,
                    pebbles_foundation::log::Cat::Gpu,
                    format_args!("layer targets in use exceed the layer pool budget (09 §3.8)"),
                );
            }
        }
        self.gpu_masks.end_frame(self.budgets.gpu_mask_bytes);
        self.gradients.lut.end_frame();
        self.image_brushes.end_frame();
        self.shader_brushes.end_frame();
        self.shader_programs.end_frame();
        self.proj_next = 0;
        stats.layer_pool_bytes = target.targets.pool.bytes();
        if !nested {
            // 10 §4.4 E4/E5: book the target; over budget is counted, never refused.
            if self.targets_book.record(target.targets, self.frame) {
                stats.frame_target_budget_exceeded += 1;
            }
            self.targets_book.end_frame(self.frame);
            self.frame += 1;
        }
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
    /// 09 §4.4 `AfterPass::CopyBackdrop(layer)`: after the parent's segment before the layer's
    /// push, copy the parent's pixels under the layer bounds into the layer's backdrop texture
    /// (the parent does not change again until the layer composites).
    fn copy_backdrop(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        layer: u32,
        layers: &[Option<super::execute::layers::LayerTarget>],
        targets: &super::frame_targets::FrameTargets,
        root: Option<&wgpu::Texture>,
        stats: &mut FrameStats,
    ) {
        use super::prepare::plan::TargetRef;
        let Some(l) = layers.get(layer as usize).and_then(Option::as_ref) else { return };
        let Some(bslot) = l.backdrop else { return };
        let desc = &self.scratch.plan.layers[layer as usize];
        let (src, origin) = match desc.parent {
            TargetRef::Root => match root {
                Some(t) => (t, (0, 0)),
                None => return,
            },
            TargetRef::Layer(p) => match layers.get(p as usize).and_then(Option::as_ref) {
                Some(pl) => (&targets.pool.textures(pl.slot).color.0, pl.segment.origin),
                None => return,
            },
        };
        let b = desc.bounds;
        stats.backdrop_copies += 1;
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: src,
                mip_level: 0,
                origin: wgpu::Origin3d { x: b.x0 - origin.0, y: b.y0 - origin.1, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &targets.pool.textures(bslot).color.0,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d { width: b.width(), height: b.height(), depth_or_array_layers: 1 },
        );
    }

    /// 09 §4.4 `BlitRoot`: the root intermediate into the frame target (`blend: None`).
    /// M30: what this frame of `target` must redraw. Backdrop composites read the root
    /// mid-frame and custom renderers paint arbitrary pixels, so either forces a full frame;
    /// the tracker still records the frame.
    fn frame_damage(
        &self,
        scene: &Scene,
        params: &FrameParams,
        targets: &mut super::FrameTargets,
        decode_srgb: bool,
        tracking: bool,
        size: (u32, u32),
    ) -> Damage {
        if !tracking {
            return Damage::Full;
        }
        let key = FrameKey {
            size,
            root_transform: params.root_transform.as_coeffs(),
            scale_factor: params.scale_factor,
            glyph_hint: params.glyph_hint,
            base_color: params.base_color.components,
            decode_srgb,
            persistent_generation: targets.root_generation(),
        };
        let damage = targets.damage_frame(scene, key);
        if self.scratch.plan.root_needs_backdrop || !self.scratch.plan.customs.is_empty() {
            Damage::Full
        } else {
            damage
        }
    }

    fn blit_root(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        group: &wgpu::BindGroup,
        dst: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        stats: &mut FrameStats,
    ) {
        use super::execute::pipelines::{PipelineKey, PipelineKind};
        let key = PipelineKey {
            kind: PipelineKind::Blit,
            color_format: format,
            sample_count: 1,
            blend: 0,
            variant: 0,
        };
        self.pipelines.ensure(device, &self.shaders, &self.layouts, key, stats);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pebbles.gpu.pass.blit"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        stats.render_passes += 1;
        let Some(pipeline) = self.pipelines.get_ensured(key) else { return };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.globals_bg, &[0, 0, 0]);
        pass.set_bind_group(1, group, &[]);
        stats.bind_group_switches += 2;
        pass.draw(0..3, 0..1);
        stats.draw_calls += 1;
    }

    /// Write this frame's SDF clip records (09 §2.4, L04), growing the clip buffer (and so the
    /// group-0 bind group) when needed. Record 0 is `ClipParams::NONE`.
    fn write_clips(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, stats: &mut FrameStats) {
        let n = self.scratch.plan.sdf_clips.len().max(1) as u64;
        if n > self.clip_slots {
            let slots = n.next_power_of_two();
            self.clip_buffer = uniform_buffer(device, "pebbles.gpu.buffer.clips", slots);
            self.clip_slots = slots;
            // Group 0 is rebuilt by `write_mask_clips` (its key includes `clip_slots`).
            stats.buffers_created += 1;
        }
        let stride = super::execute::tier_c::slot_stride(device);
        for (i, c) in self.scratch.plan.sdf_clips.iter().enumerate() {
            let p = super::gpu_types::ClipParams { rect: c.rect, radii: c.radii };
            queue.write_buffer(&self.clip_buffer, i as u64 * stride, bytemuck::bytes_of(&p));
        }
    }

    /// 09 L08: resolve every mask-clip record into the coverage atlas (stages uploads; call
    /// before the atlas flush) and keep the slots for `write_mask_clips`.
    pub(crate) fn resolve_mask_clips(&mut self, stats: &mut FrameStats) {
        self.mask_clip_slots_resolved.clear();
        self.mask_clip_slots_resolved.push(None);
        let plan = &self.scratch.plan;
        for rec in plan.mask_clips.iter().skip(1) {
            if rec.gpu.is_some() {
                self.mask_clip_slots_resolved.push(None);
                continue;
            }
            let sources = plan
                .clip_sources
                .get(rec.sources.start as usize..rec.sources.end as usize)
                .unwrap_or_default();
            let slot = match self.glyph_atlas.lookup_clip(rec.key, sources, rec.rect, stats) {
                super::execute::resources::glyph_atlas::GlyphLookup::Slot(s) => Some(s),
                _ => None,
            };
            self.mask_clip_slots_resolved.push(slot);
        }
    }

    /// 09 L08: write the mask-clip records (after the atlas flush) and rebuild group 0 when a
    /// buffer grew or the mask atlas texture was recreated.
    pub(crate) fn write_mask_clips(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        stats: &mut FrameStats,
    ) {
        let n = self.scratch.plan.mask_clips.len().max(1) as u64;
        if n > self.mask_clip_slots {
            let slots = n.next_power_of_two();
            self.mask_clip_buffer = uniform_buffer(device, "pebbles.gpu.buffer.mask_clips", slots);
            self.mask_clip_slots = slots;
            stats.buffers_created += 1;
        }
        let stride = super::execute::tier_c::slot_stride(device);
        for (i, rec) in self.scratch.plan.mask_clips.iter().enumerate() {
            let slot = self.mask_clip_slots_resolved.get(i).copied().flatten();
            #[allow(clippy::cast_precision_loss)] // device px and atlas texels ≪ 2^24
            let p = match slot {
                // A GPU mask (09 L09): texels at (0, 0) of its pool texture (`atlas.w == 2`).
                _ if rec.gpu.is_some() => super::gpu_types::MaskClipParams {
                    rect: [
                        rec.rect.x0 as f32,
                        rec.rect.y0 as f32,
                        rec.rect.width() as f32,
                        rec.rect.height() as f32,
                    ],
                    atlas: [0.0, 0.0, 0.0, 2.0],
                },
                Some(s) => super::gpu_types::MaskClipParams {
                    rect: [
                        rec.rect.x0 as f32,
                        rec.rect.y0 as f32,
                        rec.rect.width() as f32,
                        rec.rect.height() as f32,
                    ],
                    atlas: [f32::from(s.x), f32::from(s.y), f32::from(s.layer), 1.0],
                },
                // No record (index 0) or an atlas overflow: coverage 1 (the scissor still clips).
                None => super::gpu_types::MaskClipParams { rect: [0.0; 4], atlas: [0.0; 4] },
            };
            queue.write_buffer(&self.mask_clip_buffer, i as u64 * stride, bytemuck::bytes_of(&p));
        }
        let (view, generation) = self.glyph_atlas.mask_view();
        let key = (self.globals_slots, self.clip_slots, self.mask_clip_slots, generation);
        if key != self.globals_bg_key {
            self.globals_bg = globals_bind_group(
                device,
                &self.layouts,
                &self.globals_buffer,
                &self.clip_buffer,
                &self.mask_clip_buffer,
                view,
                self.gpu_masks.dummy_view(),
            );
            self.globals_bg_key = key;
            stats.bind_groups_created += 1;
        }
    }

    /// The pipeline (coverage `variant` at `format` / `samples`), group-1 bind group and dynamic
    /// offset of a brush paint (09 §5, §6, §6.1); every pipeline was ensured before the passes.
    pub(crate) fn brush_draw(
        &self,
        paint: super::prepare::plan::BrushPaint,
        variant: u8,
        format: wgpu::TextureFormat,
        samples: u32,
    ) -> Option<(&wgpu::RenderPipeline, &wgpu::BindGroup, u32)> {
        if let super::prepare::plan::BrushPaint::Shader(i) = paint {
            let (group, offset) = self.shader_brushes.binding(&self.device, i)?;
            let id = self.scratch.plan.shader_brushes.get(i as usize)?.program.id();
            return Some((self.shader_programs.pipeline(id, variant, format, samples)?, group, offset));
        }
        let (kind, group, offset) = self.brush_binding(paint)?;
        let key = super::execute::pipelines::PipelineKey {
            kind,
            color_format: format,
            sample_count: samples,
            blend: 0,
            variant,
        };
        Some((self.pipelines.get_ensured(key)?, group, offset))
    }

    /// Pipeline kind, group-1 bind group and dynamic offset of a brush paint (09 §5, §6).
    pub(crate) fn brush_binding(
        &self,
        paint: super::prepare::plan::BrushPaint,
    ) -> Option<(super::execute::pipelines::PipelineKind, &wgpu::BindGroup, u32)> {
        use super::execute::pipelines::PipelineKind;
        use super::prepare::plan::BrushPaint;
        match paint {
            BrushPaint::Gradient(i) => {
                Some((PipelineKind::Gradient, self.gradients.bind_group()?, self.gradient_offset(i)))
            }
            BrushPaint::Image(i) => {
                let (group, offset) = self.image_brushes.binding(&self.device, i)?;
                Some((PipelineKind::ImageBrush, group, offset))
            }
            BrushPaint::Shader(_) => None,
        }
    }

    /// A 1×1 white texture with the image layout (meshes without an image brush, R63).
    pub(crate) fn white_bind_group(&self) -> &wgpu::BindGroup {
        &self.white_bg
    }

    pub(crate) fn projected_buffer(&self) -> &wgpu::Buffer {
        &self.proj_buffer
    }

    /// Append one projected layer's params (09 §10.7) and return its dynamic offset.
    pub(crate) fn write_projected_params(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        h: [f32; 9],
        content: (u32, u32),
        tex: (u32, u32),
        stats: &mut FrameStats,
    ) -> u32 {
        let k = u64::from(self.proj_next);
        if k + 1 > self.proj_slots {
            // Grow before any projected bind group of the frame binds the buffer (they are
            // created after this call returns, so earlier ones are recreated below).
            self.proj_slots = (k + 1).next_power_of_two().max(4);
            self.proj_buffer = uniform_buffer(device, "pebbles.gpu.buffer.projected", self.proj_slots);
            stats.buffers_created += 1;
        }
        self.proj_next += 1;
        let (cw, ch) = (content.0 as f32, content.1 as f32);
        let map = |x: f32, y: f32| {
            let w = h[6] * x + h[7] * y + h[8];
            ((h[0] * x + h[1] * y + h[2]) / w, (h[3] * x + h[4] * y + h[5]) / w, w)
        };
        let c = [map(0.0, 0.0), map(cw, 0.0), map(0.0, ch), map(cw, ch)];
        let params = super::gpu_types::ProjectedParams {
            h: [[h[0], h[1], h[2], 0.0], [h[3], h[4], h[5], 0.0], [h[6], h[7], h[8], 0.0]],
            size: [cw, ch, tex.0 as f32, tex.1 as f32],
            corners: [[c[0].0, c[0].1, c[1].0, c[1].1], [c[2].0, c[2].1, c[3].0, c[3].1]],
            aa: [f32::from(u8::from(c.iter().all(|p| p.2 > 0.0))), 0.0, 0.0, 0.0],
        };
        let stride = super::execute::tier_c::slot_stride(device);
        queue.write_buffer(&self.proj_buffer, k * stride, bytemuck::bytes_of(&params));
        u32::try_from(k * stride).unwrap_or(0)
    }

    /// Write every effect pass's params (growing the buffer first).
    pub(crate) fn write_effect_params(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        passes: &[super::execute::effects::EffectPass],
        stats: &mut FrameStats,
    ) {
        let n = passes.len() as u64;
        if n == 0 {
            return;
        }
        if n > self.effect_slots {
            self.effect_slots = n.next_power_of_two();
            self.effect_buffer = uniform_buffer(device, "pebbles.gpu.buffer.effects", self.effect_slots);
            stats.buffers_created += 1;
        }
        let stride = super::execute::tier_c::slot_stride(device);
        for p in passes {
            queue.write_buffer(
                &self.effect_buffer,
                u64::from(p.slot) * stride,
                bytemuck::bytes_of(&p.params),
            );
        }
    }

    /// 09 §10.3: run layer `layer`'s effect passes (after its last segment).
    fn run_effect(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        layer: &super::execute::layers::LayerTarget,
        targets: &super::frame_targets::FrameTargets,
        format: wgpu::TextureFormat,
        stats: &mut FrameStats,
    ) {
        use super::execute::pipelines::PipelineKey;
        let stride = super::execute::tier_c::slot_stride(device);
        for p in &layer.effect_passes {
            let key = PipelineKey {
                kind: p.kind,
                color_format: format,
                sample_count: 1,
                blend: 0,
                variant: p.variant,
            };
            let Some(pipeline) = self.pipelines.get_ensured(key) else { continue };
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("pebbles.gpu.bind.effect"),
                layout: &self.layouts.effect,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&targets.pool.textures(p.src).color.1),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &self.effect_buffer,
                            offset: 0,
                            size: wgpu::BufferSize::new(224),
                        }),
                    },
                ],
            });
            stats.bind_groups_created += 1;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pebbles.gpu.pass.effect"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.pool.textures(p.dst).color.1,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            stats.render_passes += 1;
            pass.push_debug_group("pebbles.gpu.effect");
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &self.globals_bg, &[0, 0, 0]);
            let offset = u32::try_from(u64::from(p.slot) * stride).unwrap_or(0);
            pass.set_bind_group(1, &group, &[offset]);
            stats.bind_group_switches += 2;
            pass.set_scissor_rect(0, 0, p.dst_size.0.max(1), p.dst_size.1.max(1));
            pass.draw(0..3, 0..1);
            stats.draw_calls += 1;
            pass.pop_debug_group();
        }
    }

    /// The dynamic offset of gradient `i`'s params (09 §5.4).
    pub(crate) fn gradient_offset(&self, i: u32) -> u32 {
        super::execute::gradients::GradientResources::offset(&self.device, i)
    }

    /// The dynamic offset of mask-clip record `id` (09 L08).
    pub(crate) fn mask_clip_offset(&self, id: super::prepare::plan::MaskClipId) -> u32 {
        let stride = super::execute::tier_c::slot_stride(&self.device);
        u32::try_from(u64::from(id.0) * stride).unwrap_or(0)
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

    /// Group 0 for drawing under mask record `mask`: the variant carrying its GPU mask texture
    /// (09 L09), else the default.
    pub(crate) fn globals_bg_for(&self, mask: super::prepare::plan::MaskClipId) -> &wgpu::BindGroup {
        self.scratch
            .plan
            .mask_clips
            .get(mask.0 as usize)
            .and_then(|r| r.gpu)
            .and_then(|j| self.gpu_masks.bind_group(j))
            .unwrap_or(&self.globals_bg)
    }

    /// Group 0 with `gpu_mask` at binding 4, and the identity of the buffers it binds (rebuild
    /// a cached variant when this changes).
    pub(crate) fn globals_bg_with(
        &self,
        device: &wgpu::Device,
        gpu_mask: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        let (mask_view, _) = self.glyph_atlas.mask_view();
        globals_bind_group(
            device,
            &self.layouts,
            &self.globals_buffer,
            &self.clip_buffer,
            &self.mask_clip_buffer,
            mask_view,
            gpu_mask,
        )
    }

    pub(crate) fn globals_bg_generation(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.globals_bg_key.hash(&mut h);
        h.finish()
    }

    pub(crate) fn msaa_samples_for(&mut self, format: wgpu::TextureFormat) -> u32 {
        self.msaa.get(&self.adapter, format, None)
    }

    /// `Globals::color_flags` of the current frame (slot 0), reused by Tier C run slots.
    pub(crate) fn color_flags(&self) -> u32 {
        self.color_flags
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
            let (mask_view, _) = self.glyph_atlas.mask_view();
            let bg = globals_bind_group(
                device,
                &self.layouts,
                &buffer,
                &self.clip_buffer,
                &self.mask_clip_buffer,
                mask_view,
                self.gpu_masks.dummy_view(),
            );
            // The frame's slots are staged (`globals_staging`) and uploaded once before the
            // passes, so a grown buffer needs no copy.
            (self.globals_buffer, self.globals_bg, self.globals_slots) = (buffer, bg, n);
            self.globals_bg_key.0 = n;
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

/// A 1×1 opaque white texture and its image-layout bind group (R63).
/// Clear `view` to depth 1.0 in a depth-only pass (R62).
fn clear_depth(encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("pebbles.gpu.pass.depth_clear"),
        color_attachments: &[],
        depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
            view,
            depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Store }),
            stencil_ops: None,
        }),
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    }));
}

fn white_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layouts: &BindGroupLayouts,
) -> (wgpu::Texture, wgpu::BindGroup) {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.texture.white"),
        size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        tex.as_image_copy(),
        &[255u8; 4],
        wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: Some(1) },
        wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("pebbles.gpu.sampler.white"),
        ..Default::default()
    });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("pebbles.gpu.bind.white"),
        layout: &layouts.texture,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
        ],
    });
    (tex, bg)
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
    mask_clips: &wgpu::Buffer,
    mask_atlas: &wgpu::TextureView,
    gpu_mask: &wgpu::TextureView,
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
            wgpu::BindGroupEntry { binding: 2, resource: window(mask_clips) },
            wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(mask_atlas) },
            wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(gpu_mask) },
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
