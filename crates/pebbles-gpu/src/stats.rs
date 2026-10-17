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
}

impl Unsupported {
    /// Number of variants; must stay ≤ 64 (`WarnOnce` bits).
    pub const COUNT: usize = 15;
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

/// Per-frame counters. Reset at the top of every render; read with `Renderer::last_stats()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FrameStats {
    /// Display-list ops walked.
    pub ops: u32,
    /// Draw commands in the frame plan.
    pub cmds: u32,
    /// Draw calls issued.
    pub draw_calls: u32,
    /// Pipeline changes in the pass.
    pub pipeline_switches: u32,
    /// Render passes begun.
    pub render_passes: u32,
    /// Ops culled as offscreen.
    pub culled: u32,
    /// Glyphs rasterized this frame (atlas misses).
    pub glyphs_rasterized: u32,
    /// Bytes written to GPU buffers.
    pub bytes_uploaded: u64,
    /// CPU prepare time, microseconds.
    pub prepare_us: u32,
    /// Encode + submit time, microseconds.
    pub execute_us: u32,
    // creations — all 0 in steady state (P1)
    /// GPU buffers created.
    pub buffers_created: u32,
    /// Bind groups created.
    pub bind_groups_created: u32,
    /// Textures created.
    pub textures_created: u32,
    /// Texture views created.
    pub texture_views_created: u32,
    /// Render pipelines created.
    pub pipelines_created: u32,
    /// Images uploaded to the GPU (cache misses).
    pub images_uploaded: u32,
    /// Images dropped from the cache at end of frame.
    pub image_evictions: u32,
    /// Fallbacks taken this frame (sum of `unsupported`).
    pub fallbacks: u32,
    /// Fallbacks per kind, indexed by `Unsupported as usize`.
    pub unsupported: [u32; Unsupported::COUNT],
    /// Ops skipped for non-finite input (02 §6 input hygiene).
    pub non_finite_skipped: u32,
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
            ops: 0,
            cmds: 0,
            draw_calls: 0,
            pipeline_switches: 0,
            render_passes: 0,
            culled: 0,
            glyphs_rasterized: 0,
            bytes_uploaded: 0,
            prepare_us: 0,
            execute_us: 0,
            buffers_created: 0,
            bind_groups_created: 0,
            textures_created: 0,
            texture_views_created: 0,
            pipelines_created: 0,
            images_uploaded: 0,
            image_evictions: 0,
            fallbacks: 0,
            unsupported: [0; Unsupported::COUNT],
            non_finite_skipped: 0,
        }
    }
}
