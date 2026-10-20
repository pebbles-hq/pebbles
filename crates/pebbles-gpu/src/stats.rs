//! Per-frame renderer counters (reset every render). C12 lands the creation counters;
//! C17 adds `Unsupported`/`record`/`unsupported`; M04 completes the 10 §9.1 fields.

/// A fallback the renderer took instead of drawing an op exactly (11 §4.2 key registry).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Unsupported {
    /// Non-rect clip drawn as its bounding-box scissor.
    NonRectClip = 0,
    /// Layer opacity folded into each draw (overlaps double-blend).
    LayerAlpha,
    /// Layer with a Mix or Compose mode: its content is skipped.
    LayerBlend,
    /// Gradient on a fill/stroke: geometry skipped.
    GradientBrush,
    /// Image brush on a fill/stroke/glyph run: skipped.
    ImageBrush,
    /// Glyph run with a gradient brush, tinted with the first stop.
    GradientText,
    /// Image extend other than `Pad`, sampled clamp-to-edge.
    ImageExtend,
    /// Glyph not admitted to the atlas this frame.
    AtlasFull,
    /// Image larger than the texture limit, downscaled at upload.
    ImageTooLarge,
    /// lyon returned `Err`; the op's partial output is discarded.
    TessError,
    /// Non-finite geometry or transform; op skipped.
    NonFinite,
    /// Vertex limit reached on a device without `FULL_DRAW_INDEX_UINT32`; the rest is dropped.
    VertexLimit,
    /// An `Op` variant this renderer does not draw yet.
    UnknownOp,
    /// swash returned a subpixel mask (never requested); glyph skipped.
    GlyphContent,
    /// Unknown pixel format or byte length ≠ size; image skipped.
    ImageMalformed,
    /// Dash pattern with a negative entry or a period `<= 0`: stroked solid.
    DashDegenerate,
    /// More than `MAX_DASH_SEGMENTS` dashes: stroked solid with alpha × duty cycle.
    DashCountCapped,
    /// Stroke wider than any visible effect: width clamped to `2 · max_useful_half_width`.
    StrokeWidthClamped,
    /// A device coordinate beyond `RendererLimits::max_coord_px` (still tessellated).
    CoordOutOfRange,
    /// A color (bitmap/COLR) glyph on the outline route: skipped until R23 / R29.
    ColorGlyphTransform,
    /// A glyph id above `u16::MAX` (no font has one): glyph skipped (R22).
    GlyphIdRange,
    /// A blurred mask whose padded size exceeds `MASK_MAX_AREA`: skipped until L61 (R42).
    BlurMaskTooLarge,
    /// A clip needed a GPU coverage mask but `MAX_GPU_MASKS` were active or the mask pool budget
    /// was exhausted: clipped to its bounding box (09 §2.2 row 10).
    ClipOverflow,
    /// The gradient LUT had no room even after eviction: drawn as its first stop (09 §5.3).
    LutFull,
    /// A layer nested deeper than `RendererLimits::max_layer_depth`: flattened (11 §2.5).
    LayerDepthExceeded,
    /// More live offscreen layers than `RendererLimits::max_layer_targets`: flattened.
    LayerPoolExhausted,
    /// A mesh with a gradient brush: drawn with the first stop's color (R63).
    MeshGradient,
    /// A custom primitive whose paint type has no registered `CustomRenderer`: skipped (R60).
    CustomUnregistered,
    /// An `Op::GpuImage` whose `TextureId` is unknown or unregistered: skipped (R61).
    GpuTextureUnregistered,
    /// A content-sized GPU allocation hit out-of-memory twice (after one evict-and-retry): the
    /// 11 §2.6 degradation applied (S09).
    GpuOom,
    /// A multisampled target hit out-of-memory: single-sampled instead (S09).
    MsaaOomFallback1x,
    /// One draw's geometry alone exceeds an arena cap / the portable vertex rule: dropped (M07).
    GeometryTooLarge,
}

/// Bytes of GPU memory this Renderer requested (10 §9.3, M17), on demand — never per frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct MemoryReport {
    /// Target-owned textures of every booked target (10 §4.4).
    pub frame_targets_bytes: u64,
    /// side² × physical layers × 1.
    pub glyph_mask_bytes: u64,
    /// side² × physical layers × 4.
    pub glyph_color_bytes: u64,
    /// Σ cached image textures (10 §6).
    pub image_cache_bytes: u64,
    /// `LUT_ROW_TEXELS` × rows × 8 (`Rgba16Float`, 09 §5.3).
    pub gradient_lut_bytes: u64,
    /// The last rendered target's layer pool (09 §3.8).
    pub layer_pool_bytes: u64,
    /// The GPU clip-mask pool (09 §2.9).
    pub gpu_mask_bytes: u64,
    /// Σ arena capacities (incl. the uniform buffers).
    pub arena_bytes: u64,
    /// `Device::generate_allocator_report().total_allocated_bytes`, where the backend has one
    /// (not WebGPU / GL).
    pub driver_allocated_bytes: Option<u64>,
}

impl MemoryReport {
    /// Σ of the requested bytes (driver overhead excluded).
    pub fn total_bytes(&self) -> u64 {
        self.frame_targets_bytes
            + self.glyph_mask_bytes
            + self.glyph_color_bytes
            + self.image_cache_bytes
            + self.gradient_lut_bytes
            + self.layer_pool_bytes
            + self.gpu_mask_bytes
            + self.arena_bytes
    }
}

/// The `FrameStats` counters prepare can change (M29 saves a fragment's delta and replays it).
macro_rules! prepare_counters {
    ($m:ident) => {
        $m!(
            tier_a_draws,
            tier_b_draws,
            tier_c_draws,
            fallbacks,
            non_finite_skipped,
            degenerate_skipped,
            nine_slice_clamped
        )
    };
}

impl FrameStats {
    /// The prepare counters changed since `before` (M29).
    pub(crate) fn prepare_delta(&self, before: &Self) -> PrepareDelta {
        let mut d = PrepareDelta::default();
        macro_rules! sub_fields { ($($f:ident),*) => { $( d.$f = self.$f.wrapping_sub(before.$f); )* } }
        prepare_counters!(sub_fields);
        for (i, (a, b)) in self.unsupported.iter().zip(&before.unsupported).enumerate() {
            d.unsupported[i] = a.wrapping_sub(*b);
        }
        d
    }

    /// Re-apply a saved delta (M29 replay).
    pub(crate) fn add_prepare_delta(&mut self, d: &PrepareDelta) {
        macro_rules! add_fields { ($($f:ident),*) => { $( self.$f = self.$f.wrapping_add(d.$f); )* } }
        prepare_counters!(add_fields);
        for (a, b) in self.unsupported.iter_mut().zip(&d.unsupported) {
            *a = a.wrapping_add(*b);
        }
    }
}

/// The prepare-time `FrameStats` counters of one fragment (M29).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct PrepareDelta {
    pub tier_a_draws: u32,
    pub tier_b_draws: u32,
    pub tier_c_draws: u32,
    pub fallbacks: u32,
    pub non_finite_skipped: u32,
    pub degenerate_skipped: u32,
    pub nine_slice_clamped: u32,
    pub unsupported: [u32; Unsupported::COUNT],
}

impl Unsupported {
    /// Number of variants; must stay ≤ 64 (`WarnOnce` bits).
    pub const COUNT: usize = 32;
    /// Every variant, in discriminant order.
    pub const ALL: [Unsupported; Self::COUNT] = [
        Self::NonRectClip,
        Self::LayerAlpha,
        Self::LayerBlend,
        Self::GradientBrush,
        Self::ImageBrush,
        Self::GradientText,
        Self::ImageExtend,
        Self::AtlasFull,
        Self::ImageTooLarge,
        Self::TessError,
        Self::NonFinite,
        Self::VertexLimit,
        Self::UnknownOp,
        Self::GlyphContent,
        Self::ImageMalformed,
        Self::DashDegenerate,
        Self::DashCountCapped,
        Self::StrokeWidthClamped,
        Self::CoordOutOfRange,
        Self::ColorGlyphTransform,
        Self::GlyphIdRange,
        Self::BlurMaskTooLarge,
        Self::ClipOverflow,
        Self::LutFull,
        Self::LayerDepthExceeded,
        Self::LayerPoolExhausted,
        Self::MeshGradient,
        Self::CustomUnregistered,
        Self::GpuTextureUnregistered,
        Self::GpuOom,
        Self::MsaaOomFallback1x,
        Self::GeometryTooLarge,
    ];

    /// The variant's 11 §4.2 key, used in logs and stats dumps.
    pub fn label(self) -> &'static str {
        match self {
            Self::NonRectClip => "non-rect-clip-unsupported",
            Self::LayerAlpha => "layer-alpha-unsupported",
            Self::LayerBlend => "blend-mode-unsupported",
            Self::GradientBrush => "gradient-brush-unsupported",
            Self::ImageBrush => "image-brush-unsupported",
            Self::GradientText => "gradient-text-unsupported",
            Self::ImageExtend => "image-extend-unsupported",
            Self::AtlasFull => "glyph-atlas-full",
            Self::ImageTooLarge => "image-downscaled",
            Self::TessError => "tessellation-error",
            Self::NonFinite => "non-finite-geometry",
            Self::VertexLimit => "vertex-limit",
            Self::UnknownOp => "unknown-op",
            Self::GlyphContent => "glyph-content-unsupported",
            Self::ImageMalformed => "image-malformed",
            Self::DashDegenerate => "dash-pattern-degenerate",
            Self::DashCountCapped => "dash-count-capped",
            Self::StrokeWidthClamped => "stroke-width-clamped",
            Self::CoordOutOfRange => "coordinate-out-of-range",
            Self::ColorGlyphTransform => "color-glyph-transform-unsupported",
            Self::GlyphIdRange => "glyph-id-out-of-range",
            Self::BlurMaskTooLarge => "blur-mask-too-large",
            Self::ClipOverflow => "clip-mask-overflow",
            Self::LutFull => "gradient-lut-full",
            Self::LayerDepthExceeded => "layer-depth-exceeded",
            Self::LayerPoolExhausted => "layer-pool-exhausted",
            Self::MeshGradient => "mesh-gradient-unsupported",
            Self::CustomUnregistered => "custom-paint-unregistered",
            Self::GpuTextureUnregistered => "gpu-texture-unregistered",
            Self::GpuOom => "gpu-oom",
            Self::MsaaOomFallback1x => "msaa-oom-fallback-1x",
            Self::GeometryTooLarge => "geometry-too-large",
        }
    }
}

const _: () = assert!(Unsupported::COUNT <= 64);

/// One bit per `Unsupported` kind; lives on the Renderer (no globals, DR-12).
#[derive(Debug, Default)]
pub(crate) struct WarnOnce {
    seen: u64,
}

impl WarnOnce {
    /// Log the first frame in which `kind` occurs on this Renderer; silent afterwards.
    /// Returns whether a line was logged.
    pub(crate) fn report(&mut self, kind: Unsupported, count_this_frame: u32) -> bool {
        let bit = 1u64 << (kind as u32);
        if count_this_frame == 0 || self.seen & bit != 0 {
            return false;
        }
        self.seen |= bit;
        pebbles_foundation::log::logf(
            pebbles_foundation::log::Level::Warn,
            pebbles_foundation::log::Cat::Gpu,
            format_args!(
                "wgpu renderer fallback: {} ({count_this_frame} this frame) — logged once",
                kind.label()
            ),
        );
        true
    }
}

/// Per-frame renderer counters (10 §9.1). Reset at the top of every render call; read with
/// `Renderer::last_frame_stats()`. Cheap (integer adds + a few `Instant::now()`).
#[derive(Clone, Copy, Debug, PartialEq)] // `Default` is hand-written (std arrays only up to 32)
#[non_exhaustive]
pub struct FrameStats {
    // work
    /// Draw calls issued.
    pub draw_calls: u32,
    /// `set_pipeline` with a different pipeline than the previous.
    pub pipeline_switches: u32,
    /// `set_bind_group` calls.
    pub bind_group_switches: u32,
    /// `set_scissor_rect` calls.
    pub scissor_changes: u32,
    /// Render passes begun.
    pub render_passes: u32,
    /// Path segments tessellated (plan).
    pub segments: u32,
    /// Vertices in the plan's solid arena.
    pub vertices: u32,
    /// Indices in the plan's solid arena.
    pub indices: u32,
    /// Instances (shapes, shadows, glyphs, images) in the plan.
    pub instances: u32,
    // uploads
    /// Bytes written to GPU buffers.
    pub buffer_bytes_uploaded: u64,
    /// Bytes written to GPU textures.
    pub texture_bytes_uploaded: u64,
    /// Glyphs rasterized this frame (atlas misses).
    pub glyphs_rasterized: u32,
    /// Tier B path + clip masks rasterized this frame (R15, L08).
    pub masks_rasterized: u32,
    // raster tiers (DR-20, R16)
    /// Ops drawn as Tier A analytic shapes.
    pub tier_a_draws: u32,
    /// Ops drawn as Tier B coverage masks.
    pub tier_b_draws: u32,
    /// Ops drawn as Tier C meshes.
    pub tier_c_draws: u32,
    /// Tier C runs drawn without MSAA (4× unsupported).
    pub tier_c_aliased: u32,
    /// Images uploaded (cache misses).
    pub images_uploaded: u32,
    /// Gradient LUT rows uploaded (L34).
    pub gradient_rows_uploaded: u32,
    // creations — all 0 in steady state (P1)
    /// GPU buffers created.
    pub buffers_created: u32,
    /// Textures created.
    pub textures_created: u32,
    /// Texture views created.
    pub texture_views_created: u32,
    /// Bind groups created.
    pub bind_groups_created: u32,
    /// Samplers created.
    pub samplers_created: u32,
    /// Render pipelines created.
    pub pipelines_created: u32,
    // caches
    /// Mask atlas: occupied px × 100 / (side² × active layers).
    pub glyph_atlas_occupancy_pct: u8,
    /// Color atlas occupancy, percent.
    pub color_atlas_occupancy_pct: u8,
    /// Glyphs evicted from the atlas.
    pub glyph_evictions: u32,
    /// Glyphs that found no atlas room.
    pub glyph_overflow: u32,
    /// Atlas side doublings / layer additions.
    pub atlas_grows: u32,
    /// Bytes held by the image cache.
    pub image_cache_bytes: u64,
    /// Images dropped from the cache at end of frame.
    pub image_evictions: u32,
    // hygiene
    /// = sum of `unsupported`; only `FrameStats::record` changes it (06 §4.6).
    pub fallbacks: u32,
    /// Per-kind fallback counts, indexed by `Unsupported as usize` (keys in 11 §4.2).
    pub unsupported: [u32; Unsupported::COUNT],
    /// Ops dropped for exceeding a device limit (§2.5).
    pub oversize_ops_dropped: u32,
    /// Ops skipped for non-finite input (02 §6 input hygiene).
    pub non_finite_skipped: u32,
    /// Ops skipped silently for a singular transform (11 §2.3, S02).
    pub degenerate_skipped: u32,
    // counters owned by later cards
    /// GPU clip masks rendered (09 L09).
    pub gpu_masks: u32,
    /// GPU masks drawn without MSAA (09 §2.9).
    pub gpu_masks_aliased: u32,
    /// Backdrop copies for blend modes (09 L24).
    pub backdrop_copies: u32,
    /// Layers whose alpha folded into their single draw (09 §3.4).
    pub layers_folded: u32,
    /// Layers rendered offscreen and composited (09 §3.5).
    pub layers_offscreen: u32,
    /// Bytes held by this target's layer pool (09 §3.8).
    pub layer_pool_bytes: u64,
    /// Layer pool over its budget after the end-of-frame trim (09 §3.8).
    pub layer_budget_exceeded: u32,
    /// Gradient ramps baked on the CPU this frame (09 §5.3).
    pub lut_bakes: u32,
    /// COLR glyphs rendered into the color atlas (02 R39).
    pub color_glyph_renders: u32,
    /// Nine-slice centers clamped into their image (02 R36).
    pub nine_slice_clamped: u32,
    /// Ops re-prepared this frame (M29 retained prepare).
    pub ops_prepared: u32,
    /// Damaged pixels redrawn (M30).
    pub damage_px: u64,
    /// Frames redrawn in full (M30).
    pub full_redraws: u32,
    /// §4.4 E5.
    pub frame_target_budget_exceeded: u32,
    /// Threaded live resize timed out (08 §17.4).
    pub resize_sync_timeouts: u32,
    /// App shader-brush programs that failed to compile (09 L43).
    pub shader_errors: u32,
    /// Display-list ops walked (C07).
    pub ops: u32,
    /// Draw commands in the frame plan (C07).
    pub cmds: u32,
    /// Ops culled as offscreen (R04).
    pub culled: u32,
    // timing, µs (web_time::Instant)
    /// CPU prepare.
    pub prepare_us: u32,
    /// SYNC step 2 + arena uploads.
    pub upload_us: u32,
    /// Encoding.
    pub encode_us: u32,
    /// Queue submit.
    pub submit_us: u32,
    /// M22; `None` without TIMESTAMP_QUERY (always `None` on WebGL2).
    pub gpu_us: Option<u32>,
}

impl FrameStats {
    /// The one way to record a fallback (11 §4.2).
    pub(crate) fn record(&mut self, kind: Unsupported) {
        self.unsupported[kind as usize] += 1;
        self.fallbacks += 1;
    }
}

#[allow(clippy::derivable_impls)] // hand-written per 10 §9.1
impl Default for FrameStats {
    fn default() -> Self {
        FrameStats {
            draw_calls: 0,
            pipeline_switches: 0,
            bind_group_switches: 0,
            scissor_changes: 0,
            render_passes: 0,
            segments: 0,
            vertices: 0,
            indices: 0,
            instances: 0,
            buffer_bytes_uploaded: 0,
            texture_bytes_uploaded: 0,
            glyphs_rasterized: 0,
            masks_rasterized: 0,
            tier_a_draws: 0,
            tier_b_draws: 0,
            tier_c_draws: 0,
            tier_c_aliased: 0,
            images_uploaded: 0,
            gradient_rows_uploaded: 0,
            buffers_created: 0,
            textures_created: 0,
            texture_views_created: 0,
            bind_groups_created: 0,
            samplers_created: 0,
            pipelines_created: 0,
            glyph_atlas_occupancy_pct: 0,
            color_atlas_occupancy_pct: 0,
            glyph_evictions: 0,
            glyph_overflow: 0,
            atlas_grows: 0,
            image_cache_bytes: 0,
            image_evictions: 0,
            fallbacks: 0,
            unsupported: [0; Unsupported::COUNT],
            oversize_ops_dropped: 0,
            non_finite_skipped: 0,
            degenerate_skipped: 0,
            gpu_masks: 0,
            gpu_masks_aliased: 0,
            backdrop_copies: 0,
            layers_folded: 0,
            layers_offscreen: 0,
            layer_pool_bytes: 0,
            layer_budget_exceeded: 0,
            lut_bakes: 0,
            color_glyph_renders: 0,
            nine_slice_clamped: 0,
            ops_prepared: 0,
            damage_px: 0,
            full_redraws: 0,
            frame_target_budget_exceeded: 0,
            resize_sync_timeouts: 0,
            shader_errors: 0,
            ops: 0,
            cmds: 0,
            culled: 0,
            prepare_us: 0,
            upload_us: 0,
            encode_us: 0,
            submit_us: 0,
            gpu_us: None,
        }
    }
}

impl std::fmt::Display for FrameStats {
    /// The exact one-line §9.1 format printed on the runner's `[pebbles gpu]` line.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ms = |us: u32| f64::from(us) / 1000.0;
        write!(
            f,
            "prepare={:.2}ms upload={:.2}ms encode={:.2}ms submit={:.2}ms gpu=",
            ms(self.prepare_us),
            ms(self.upload_us),
            ms(self.encode_us),
            ms(self.submit_us)
        )?;
        match self.gpu_us {
            Some(us) => write!(f, "{:.2}ms", ms(us))?,
            None => write!(f, "n/a")?,
        }
        #[allow(clippy::cast_precision_loss)] // display only
        let img = self.image_cache_bytes as f64 / 1_048_576.0;
        write!(
            f,
            " draws={} switches={} passes={} segs={} verts={} inst={} buf_up={}KiB tex_up={}KiB raster={} \
             new=buf:{},tex:{},view:{},bg:{},smp:{},pipe:{} atlas={}%/{}% evict={} overflow={} img={:.1}MiB fallbacks={}",
            self.draw_calls,
            self.pipeline_switches,
            self.render_passes,
            self.segments,
            self.vertices,
            self.instances,
            self.buffer_bytes_uploaded / 1024,
            self.texture_bytes_uploaded / 1024,
            self.glyphs_rasterized,
            self.buffers_created,
            self.textures_created,
            self.texture_views_created,
            self.bind_groups_created,
            self.samplers_created,
            self.pipelines_created,
            self.glyph_atlas_occupancy_pct,
            self.color_atlas_occupancy_pct,
            self.glyph_evictions,
            self.glyph_overflow,
            img,
            self.fallbacks
        )?;
        // Phase 4 (09 L52): backdrop copies, LUT bakes / rows uploaded, layer pool KiB.
        write!(
            f,
            " backdrop={} lut={}/{} pool={}KiB",
            self.backdrop_copies,
            self.lut_bakes,
            self.gradient_rows_uploaded,
            self.layer_pool_bytes / 1024
        )
    }
}

#[cfg(test)]
mod display_tests {
    use super::FrameStats;

    #[test]
    fn display_format_is_stable() {
        let s = FrameStats {
            prepare_us: 420,
            upload_us: 30,
            encode_us: 110,
            submit_us: 50,
            gpu_us: None,
            draw_calls: 37,
            pipeline_switches: 9,
            render_passes: 1,
            segments: 1,
            vertices: 12034,
            instances: 842,
            buffer_bytes_uploaded: 196 * 1024,
            texture_bytes_uploaded: 0,
            glyphs_rasterized: 0,
            glyph_atlas_occupancy_pct: 12,
            color_atlas_occupancy_pct: 0,
            image_cache_bytes: 12_897_485,
            ..FrameStats::default()
        };
        assert_eq!(
            s.to_string(),
            "prepare=0.42ms upload=0.03ms encode=0.11ms submit=0.05ms gpu=n/a draws=37 switches=9 passes=1 \
             segs=1 verts=12034 inst=842 buf_up=196KiB tex_up=0KiB raster=0 new=buf:0,tex:0,view:0,bg:0,smp:0,pipe:0 \
             atlas=12%/0% evict=0 overflow=0 img=12.3MiB fallbacks=0 backdrop=0 lut=0/0 pool=0KiB"
        );
        let timed = FrameStats { gpu_us: Some(1234), ..FrameStats::default() };
        assert!(timed.to_string().contains("gpu=1.23ms"));
    }
}
