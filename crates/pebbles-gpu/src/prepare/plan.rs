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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ClipState {
    pub scissor: ScissorRect,
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
    /// contiguous ranges. `Images` never merges (until C13). Returns whether a new command
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
    }
}

#[cfg(test)]
fn clip_str(clip: &ClipState) -> String {
    let s = clip.scissor;
    format!("clip=[{},{},{},{}]", s.x0, s.y0, s.x1, s.y1)
}

/// Merge `last` + `new` into one command when the exact merge rule holds, else `None`.
fn merge(last: &DrawCmd, new: &DrawCmd, images: &[ImageRequest]) -> Option<DrawCmd> {
    // Images never merge until C13 adds `ImageKey`.
    if matches!(new, DrawCmd::Images { .. }) {
        return None;
    }
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
