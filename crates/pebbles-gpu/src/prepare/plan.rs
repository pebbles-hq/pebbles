//! The frame plan: the ordered `DrawCmd` stream, geometry arenas and the merge rule.

use pebbles_render::paint::Glyph;
use peniko::FontData;

use super::num::{ceil_px, floor_px};
use crate::gpu_types::{ImageVertex, ShadowInstance, SolidVertex};

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
    /// Instanced shadow quads from `shadows`.
    Shadows { instances: std::ops::Range<u32>, clip: ClipState },
    /// Glyph runs from `glyph_runs` (mask and color).
    Glyphs { glyphs: std::ops::Range<u32>, clip: ClipState },
    /// Image quads from `images`; never merges until C13 adds `ImageKey`.
    Images { images: std::ops::Range<u32>, clip: ClipState },
}

/// A shaped glyph run, transformed to physical-pixel space, ready for the glyph atlas.
#[derive(Clone)]
pub(crate) struct GlyphRun {
    pub font: FontData,
    pub font_size: f32,
    pub font_weight: u16,
    pub color: [f32; 4],
    pub glyphs: Vec<Glyph>,
    pub synthesize_italic: bool,
}

/// One image draw: the image index (into the scene's uploads) + a quad in physical-pixel space.
#[derive(Clone, Copy)]
pub(crate) struct ImageQuad {
    pub image_index: u32,
    pub vertices: [ImageVertex; 4],
}

/// Plan-local counters (dedicated counters only — fallbacks are recorded through
/// `FrameStats::record`). C07 lands these four fields; later cards extend the struct.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PrepareStats {
    pub ops: u32,
    pub culled: u32,
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
    pub shadows: Vec<ShadowInstance>,
    pub glyph_runs: Vec<GlyphRun>,
    pub images: Vec<ImageQuad>,
    // the ordered stream
    pub cmds: Vec<DrawCmd>,
    pub stats: PrepareStats,
    /// Visit index of the op that produced each cmd (test builds only; order invariant).
    #[cfg(test)]
    pub cmd_ops: Vec<u32>,
}

impl FramePlan {
    pub(crate) fn clear(&mut self) {
        self.solid_vertices.clear();
        self.solid_indices.clear();
        self.shadows.clear();
        self.glyph_runs.clear();
        self.images.clear();
        self.cmds.clear();
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
            && let Some(merged) = merge(last, &cmd)
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
        let glyphs: usize = self.glyph_runs.iter().map(|r| r.glyphs.len()).sum();
        let mut out = String::new();
        let _ = writeln!(out, "# pebbles-gpu FramePlan v1");
        let _ = writeln!(out, "target {w}x{h}");
        let _ = writeln!(
            out,
            "arenas solid_v={} solid_i={} shapes=0 shadows={} glyphs={glyphs} runs={} images={} layers=0 gradients=0",
            self.solid_vertices.len(),
            self.solid_indices.len(),
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
fn merge(last: &DrawCmd, new: &DrawCmd) -> Option<DrawCmd> {
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
        _ => None,
    }
}
