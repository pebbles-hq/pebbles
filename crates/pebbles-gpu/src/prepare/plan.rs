//! The frame plan: the ordered `DrawCmd` stream, geometry arenas and the merge rule.

use peniko::FontData;

use super::num::{ceil_px, floor_px};
use crate::execute::resources::atlas_alloc::CoverageKey;
use crate::execute::resources::image_cache::{ImageKey, SamplerKey};
use crate::gpu_types::{QuadInstance, ShadowInstance, ShapeInstance, SolidVertex};

/// Device-pixel rectangle `[x0, x1) × [y0, y1)`, stored as corners so intersection never
/// adds (no overflow).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct ScissorRect {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl ScissorRect {
    /// The full-target scissor.
    pub(crate) fn full(target: (u32, u32)) -> Self {
        Self { x0: 0, y0: 0, x1: target.0, y1: target.1 }
    }

    /// Floor/ceil device bounds into the target; `None` if NaN or empty.
    pub(crate) fn from_device_bounds(x0: f64, y0: f64, x1: f64, y1: f64, target: (u32, u32)) -> Option<Self> {
        let (x0, y0) = (floor_px(x0, target.0)?, floor_px(y0, target.1)?);
        let (x1, y1) = (ceil_px(x1, target.0)?, ceil_px(y1, target.1)?);
        (x0 < x1 && y0 < y1).then_some(Self { x0, y0, x1, y1 })
    }

    /// Corner max/min intersection. `None` when empty.
    pub(crate) fn intersect(self, other: Self) -> Option<Self> {
        let s = Self {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        };
        (s.x0 < s.x1 && s.y0 < s.y1).then_some(s)
    }

    /// `x1 - x0` (invariant `x0 <= x1`).
    pub(crate) fn width(self) -> u32 {
        self.x1 - self.x0
    }

    /// `y1 - y0`.
    pub(crate) fn height(self) -> u32 {
        self.y1 - self.y0
    }
}

/// The clip a drawing command executes under. Every drawing `DrawCmd` carries its own copy;
/// prepare keeps the current one on its `ClipStack`. C07 lands `scissor` only (L03 adds `sdf`,
/// L08 adds `mask`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct ClipState {
    /// Always set; the full target when unclipped. Never empty in an emitted cmd.
    pub scissor: ScissorRect,
    /// The one active SDF clip, or `SdfClipId::NONE` (09 §2.4, L04).
    pub sdf: SdfClipId,
    /// The one active coverage-mask clip, or `MaskClipId::NONE` (09 L08/L09).
    pub mask: MaskClipId,
}

impl ClipState {
    /// No clip but the full target.
    pub(crate) fn full(target: (u32, u32)) -> Self {
        Self { scissor: ScissorRect::full(target), sdf: SdfClipId::NONE, mask: MaskClipId::NONE }
    }
}

/// Index into `FramePlan::sdf_clips`. `SdfClipId::NONE` (0) is the "no SDF clip" record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct SdfClipId(pub(crate) u32);
impl SdfClipId {
    pub(crate) const NONE: SdfClipId = SdfClipId(0);
}

/// Index into `FramePlan::mask_clips` (L08). `MaskClipId::NONE` (0) = no mask clip.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct MaskClipId(pub(crate) u32);
impl MaskClipId {
    pub(crate) const NONE: MaskClipId = MaskClipId(0);
}

/// One clip path of a mask chain (09 L08): rasterized exactly over the record's rect.
#[derive(Clone, Debug)]
pub(crate) struct MaskClipSource {
    pub path: kurbo::BezPath,
    /// Full device transform.
    pub xf: kurbo::Affine,
    pub rule: peniko::Fill,
}

/// An active coverage-mask clip (09 L08): the product of every clip in `sources` over `rect`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MaskClipRecord {
    /// Identity of the chain (every clip's geometry, transform and rule).
    pub key: u64,
    /// Range into `FramePlan::clip_sources` (outermost first).
    pub sources: std::ops::Range<u32>,
    /// Device px; the scissor of the state that carries it.
    pub rect: ScissorRect,
    /// A GPU coverage mask (09 §2.9, L09): index into `FramePlan::gpu_mask_jobs`. `None`: a
    /// Tier B atlas mask (L08).
    pub gpu: Option<u32>,
    /// GPU masks in this record's chain (the active count for `MAX_GPU_MASKS`).
    pub gpu_depth: u8,
}

/// One GPU coverage mask to render before the frame's first pass (09 §2.9).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct GpuMaskJob {
    /// Device rect of the mask texture.
    pub rect: ScissorRect,
    /// The clip path's lyon fill mesh in device px: a range of the solid index arena.
    pub indices: std::ops::Range<u32>,
    /// The mask active when this clip was pushed (`MaskClipId::NONE`: none); multiplied in.
    pub parent: MaskClipId,
}

/// A rounded-rect clip evaluated analytically in every content shader (09 §2.4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SdfClip {
    /// Device px: x0, y0, x1, y1.
    pub rect: [f32; 4],
    /// Device px: top-left, top-right, bottom-right, bottom-left. All 0 for a sharp rect.
    pub radii: [f32; 4],
}

impl SdfClip {
    /// Record 0: a rect so large that coverage is 1 everywhere.
    pub(crate) const NONE: SdfClip = SdfClip { rect: [-1.0e8, -1.0e8, 1.0e8, 1.0e8], radii: [0.0; 4] };
}

/// One ordered command. Drawing variants reference arena ranges and carry the `ClipState`
/// they execute under; there are no clip commands (C11/L10 add the rest).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DrawCmd {
    /// Indexed triangles from `solid_indices` (base_vertex is always 0 — indices are pre-rebased).
    Solid { indices: std::ops::Range<u32>, clip: ClipState },
    /// Instanced analytic rect/rrect/circle shapes from `shapes` (R10, Tier A).
    Shapes { instances: std::ops::Range<u32>, clip: ClipState },
    /// Instanced shadow quads from `shadows`.
    Shadows { instances: std::ops::Range<u32>, clip: ClipState },
    /// Instanced glyph quads (mask and color, one pipeline) from `glyphs`.
    Glyphs { glyphs: std::ops::Range<u32>, clip: ClipState },
    /// Instanced image quads from `images`; merges only with the same `ImageKey` + `SamplerKey`.
    Images { images: std::ops::Range<u32>, clip: ClipState },
    /// Start rendering into layer `layer`'s target (09 §3.5; ends the parent's pass).
    PushLayer { layer: u32 },
    /// End layer `layer`'s target.
    PopLayer { layer: u32 },
    /// Composite layer `layer` into the parent target under `clip` (09 §3.7).
    Composite { layer: u32, clip: ClipState },
}

/// Which target a layer composites into (09 §3.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetRef {
    Root,
    Layer(u32),
}

/// How a layer composites (09 §4.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompositePath {
    Over,
    #[allow(dead_code, reason = "constructed once non-Over blends route offscreen (L21)")]
    Compose,
    Backdrop,
}

/// An offscreen layer (09 §3.5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LayerDesc {
    /// Device px, non-empty: round_out(own clip bbox) ∩ parent scissor ∩ target.
    pub bounds: ScissorRect,
    /// (0, 1]; 1.0 for promoted layers.
    pub alpha: f32,
    pub mix: peniko::Mix,
    pub compose: peniko::Compose,
    pub path: CompositePath,
    pub parent: TargetRef,
}

/// One glyph run's shared data; `GlyphRequest::run` indexes this.
#[derive(Clone)]
pub(crate) struct GlyphRunInfo {
    /// Font bytes (`Blob` clone = refcount bump). Released at end of frame.
    pub font: FontData,
    /// Range into `FramePlan::coords`.
    pub coords: std::ops::Range<u32>,
    /// Device raster size in px.
    pub size_px: f32,
}

/// One glyph to draw from an atlas. Resolved 1:1 into a `QuadInstance` by `execute/resolve.rs`
/// (a glyph that cannot be rasterized or placed becomes a zero-area quad so ranges stay valid).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GlyphRequest {
    /// A glyph, or a Tier B path mask (R15) — both live in the coverage atlas.
    pub key: CoverageKey,
    /// Glyphs: index into `glyph_runs`; path masks: index into `mask_sources`.
    pub run: u32,
    /// Integer device-pixel pen origin; the fractional x lives in `key.subpx_x` (02 §7.4). Path
    /// masks: the integer part of the device translation (the fraction is in the key).
    pub origin: [f32; 2],
    /// Premultiplied tint (mask glyphs); color glyphs use only `.a` as opacity.
    pub color: [f32; 4],
    /// Outline-route bitmap glyphs (R23 step 2a): the run's linear map `[a, b, c, d]` (kurbo
    /// order) — the native strike is drawn as a transformed quad at the unsnapped device pen
    /// `origin`, scaled `font_size / strike_ppem`. `None`: an axis-aligned atlas quad.
    pub linear: Option<[f32; 4]>,
}

/// One image to draw.
#[derive(Clone)]
pub(crate) struct ImageRequest {
    pub key: ImageKey,
    pub sampler: SamplerKey,
    /// `Blob` clone (refcount bump) — never a pixel copy. Released at end of frame.
    pub image: peniko::ImageData,
    /// Device-space placement and opacity.
    pub quad: QuadInstance,
    /// `Medium`/`High` images carry the R35 mip chain.
    pub mips: bool,
    /// Dynamic images (R37): the frame to make resident (`image` mirrors its pixels).
    pub dynamic: Option<pebbles_render::paint::DynamicImageFrame>,
}

/// Plan-local counters (dedicated counters only — fallbacks are recorded through
/// `FrameStats::record`). C07 lands these four fields; later cards extend the struct.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PrepareStats {
    pub ops: u32,
    pub culled: u32,
    /// Ops culled because their device bounds miss the active scissor (R04; debug builds).
    #[cfg(debug_assertions)]
    pub culled_ops: u32,
    pub fast_rects: u32,
    pub tess_calls: u32,
    /// Glyphs culled against the active scissor before any atlas work (R25).
    pub culled_glyphs: u32,
    /// Clip decisions (09 §2.2, §2.7).
    pub clips_scissor: u32,
    pub clips_noop: u32,
    pub clips_culled: u32,
    pub clips_sdf: u32,
    pub clips_mask: u32,
    pub clips_gpu_mask: u32,
    pub gpu_mask_overflow: u32,
    pub nonfinite: u32,
    pub unbalanced_pops: u32,
    pub autoclosed: u32,
    /// Layer plans (09 §3.3).
    pub layers_skipped: u32,
    pub layers_pure_clip: u32,
    pub layers_folded: u32,
    pub layers_offscreen: u32,
    pub layers_promoted: u32,
}

/// Everything one frame draws, in order. Reused across frames: `clear()` keeps every capacity.
#[derive(Default)]
pub(crate) struct FramePlan {
    pub target_size: (u32, u32),
    // geometry arenas (each uploaded ONCE per frame)
    pub solid_vertices: Vec<SolidVertex>,
    pub solid_indices: Vec<u32>,
    pub shapes: Vec<ShapeInstance>,
    pub shadows: Vec<ShadowInstance>,
    pub glyph_runs: Vec<GlyphRunInfo>,
    pub glyphs: Vec<GlyphRequest>,
    /// What to rasterize on a path-mask atlas miss (R15), indexed by `GlyphRequest::run`.
    pub mask_sources: Vec<super::path_masks::MaskSource>,
    /// Normalized variation coords of every run, indexed by `GlyphRunInfo::coords`.
    pub coords: Vec<i16>,
    pub images: Vec<ImageRequest>,
    /// SDF clip records (09 §2.4); `[0]` is `SdfClip::NONE`.
    pub sdf_clips: Vec<SdfClip>,
    /// Mask clip records (09 L08); `[0]` is the "no mask" record.
    pub mask_clips: Vec<MaskClipRecord>,
    /// The clip paths the mask records reference.
    pub clip_sources: Vec<MaskClipSource>,
    /// GPU coverage masks (09 §2.9), parents before children.
    pub gpu_mask_jobs: Vec<GpuMaskJob>,
    /// Estimated mask pool bytes of `gpu_mask_jobs` (against `GpuBudgets::gpu_mask_bytes`).
    pub gpu_mask_bytes: u64,
    /// Offscreen layers (09 §3.5), indexed by `DrawCmd::{PushLayer, PopLayer, Composite}`.
    pub layers: Vec<LayerDesc>,
    // the ordered stream
    pub cmds: Vec<DrawCmd>,
    /// OC rows (R01) that fell back in this plan.
    pub fallback_features: super::coverage::FeatureSet,
    pub stats: PrepareStats,
    /// Visit index of the op that produced each cmd (test builds only; order invariant).
    #[cfg(test)]
    pub cmd_ops: Vec<u32>,
}

impl FramePlan {
    pub(crate) fn clear(&mut self) {
        self.solid_vertices.clear();
        self.solid_indices.clear();
        self.shapes.clear();
        self.shadows.clear();
        self.glyph_runs.clear();
        self.glyphs.clear();
        self.mask_sources.clear();
        self.coords.clear();
        self.images.clear();
        self.sdf_clips.clear();
        self.sdf_clips.push(SdfClip::NONE);
        self.mask_clips.clear();
        self.mask_clips.push(MaskClipRecord {
            key: 0,
            sources: 0..0,
            rect: ScissorRect::default(),
            gpu: None,
            gpu_depth: 0,
        });
        self.clip_sources.clear();
        self.gpu_mask_jobs.clear();
        self.gpu_mask_bytes = 0;
        self.layers.clear();
        self.cmds.clear();
        self.fallback_features = super::coverage::FeatureSet::default();
        self.stats = PrepareStats::default();
        #[cfg(test)]
        self.cmd_ops.clear();
    }

    /// Drop the `FontData` (Blob) clones held by glyph runs (called at end of frame).
    pub(crate) fn release_refs(&mut self) {
        self.glyph_runs.clear();
        self.images.clear();
    }

    /// Apply the merge rule: extend `cmds.last_mut()` iff same variant, same clip and
    /// contiguous ranges (`Images` also need the same `ImageKey` + `SamplerKey`). Returns whether a new command
    /// was pushed (true) or merged (false).
    pub(crate) fn push_draw(&mut self, cmd: DrawCmd) -> bool {
        if let Some(last) = self.cmds.last_mut()
            && let Some(merged) = merge(last, &cmd, &self.images)
        {
            *last = merged;
            return false;
        }
        self.cmds.push(cmd);
        true
    }

    /// Deterministic text of the plan (06 §10.3 v1 format).
    #[cfg(test)]
    pub(crate) fn debug_dump(&self) -> String {
        use std::fmt::Write;
        let (w, h) = self.target_size;
        let glyphs = self.glyphs.len();
        let mut out = String::new();
        let _ = writeln!(out, "# pebbles-gpu FramePlan v1");
        let _ = writeln!(out, "target {w}x{h}");
        let _ = writeln!(
            out,
            "arenas solid_v={} solid_i={} shapes={} shadows={} glyphs={glyphs} runs={} images={} layers=0 gradients=0",
            self.solid_vertices.len(),
            self.solid_indices.len(),
            self.shapes.len(),
            self.shadows.len(),
            self.glyph_runs.len(),
            self.images.len(),
        );
        for (i, cmd) in self.cmds.iter().enumerate() {
            let _ = writeln!(out, "{i} {}", format_cmd(cmd));
        }
        let _ = writeln!(
            out,
            "stats ops={} culled={} fast_rects={} unsupported=-",
            self.stats.ops, self.stats.culled, self.stats.fast_rects
        );
        out
    }
}

/// Format one command line (the `<Variant> <fields> clip=[…]` part).
#[cfg(test)]
fn format_cmd(cmd: &DrawCmd) -> String {
    match cmd {
        DrawCmd::Solid { indices, clip } => {
            format!("Solid idx={}..{} {}", indices.start, indices.end, clip_str(clip))
        }
        DrawCmd::Shapes { instances, clip } => {
            format!("Shapes inst={}..{} {}", instances.start, instances.end, clip_str(clip))
        }
        DrawCmd::Shadows { instances, clip } => {
            format!("Shadows inst={}..{} {}", instances.start, instances.end, clip_str(clip))
        }
        DrawCmd::Glyphs { glyphs, clip } => {
            format!("Glyphs glyphs={}..{} {}", glyphs.start, glyphs.end, clip_str(clip))
        }
        DrawCmd::Images { images, clip } => {
            format!("Images images={}..{} {}", images.start, images.end, clip_str(clip))
        }
        DrawCmd::PushLayer { layer } => format!("PushLayer {layer}"),
        DrawCmd::PopLayer { layer } => format!("PopLayer {layer}"),
        DrawCmd::Composite { layer, clip } => format!("Composite {layer} {}", clip_str(clip)),
    }
}

#[cfg(test)]
fn clip_str(clip: &ClipState) -> String {
    let s = clip.scissor;
    format!("clip=[{},{},{},{}]", s.x0, s.y0, s.x1, s.y1)
}

/// Merge `last` + `new` into one command when the exact merge rule holds, else `None`.
fn merge(last: &DrawCmd, new: &DrawCmd, images: &[ImageRequest]) -> Option<DrawCmd> {
    if std::mem::discriminant(last) != std::mem::discriminant(new) {
        return None;
    }
    match (last, new) {
        (DrawCmd::Solid { indices: a, clip: ca }, DrawCmd::Solid { indices: b, clip: cb })
            if ca == cb && a.end == b.start =>
        {
            Some(DrawCmd::Solid { indices: a.start..b.end, clip: *ca })
        }
        (DrawCmd::Shapes { instances: a, clip: ca }, DrawCmd::Shapes { instances: b, clip: cb })
            if ca == cb && a.end == b.start =>
        {
            Some(DrawCmd::Shapes { instances: a.start..b.end, clip: *ca })
        }
        (DrawCmd::Shadows { instances: a, clip: ca }, DrawCmd::Shadows { instances: b, clip: cb })
            if ca == cb && a.end == b.start =>
        {
            Some(DrawCmd::Shadows { instances: a.start..b.end, clip: *ca })
        }
        (DrawCmd::Glyphs { glyphs: a, clip: ca }, DrawCmd::Glyphs { glyphs: b, clip: cb })
            if ca == cb && a.end == b.start =>
        {
            Some(DrawCmd::Glyphs { glyphs: a.start..b.end, clip: *ca })
        }
        (DrawCmd::Images { images: a, clip: ca }, DrawCmd::Images { images: b, clip: cb })
            if ca == cb && a.end == b.start && same_texture(images, a.start, b.start) =>
        {
            Some(DrawCmd::Images { images: a.start..b.end, clip: *ca })
        }
        _ => None,
    }
}

/// Whether image requests `a` and `b` sample the same texture with the same sampler.
fn same_texture(images: &[ImageRequest], a: u32, b: u32) -> bool {
    match (images.get(a as usize), images.get(b as usize)) {
        (Some(x), Some(y)) => x.key == y.key && x.sampler == y.sampler,
        _ => false,
    }
}
