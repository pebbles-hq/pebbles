//! Fill tessellation: one reusable lyon `FillTessellator` fed device-space path events
//! directly (no `lyon::path::Path`), appending to the solid arena.

use std::ops::Range;

use kurbo::{Affine, PathEl};
use lyon_tessellation::math::{Point, point};
use lyon_tessellation::path::PathEvent;
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillTessellator, FillVertex, FillVertexConstructor, TessellationError,
    VertexBuffers,
};

use super::num::{index_u32, narrow_f32};
use super::plan::FramePlan;
use crate::gpu_types::SolidVertex;
use crate::stats::{FrameStats, Unsupported};

/// Tessellation tolerance for screen-space geometry (physical px).
pub(crate) const FILL_TOLERANCE_PX: f32 = 0.1;

/// Reusable tessellation state, owned by the Renderer (via `PrepareScratch`) and reused
/// across ops and frames: buffers are cleared per op, their capacity kept.
pub(crate) struct PathTessellator {
    pub fill: FillTessellator,
    pub stroke_ctx: kurbo::StrokeCtx,
    out: VertexBuffers<SolidVertex, u32>,
    events: Vec<PathEvent>,
}

impl Default for PathTessellator {
    fn default() -> Self {
        Self {
            fill: FillTessellator::new(),
            stroke_ctx: kurbo::StrokeCtx::default(),
            out: VertexBuffers::new(),
            events: Vec::new(),
        }
    }
}

/// The lyon options for a fill rule. The rule is always explicit (never lyon's default options).
pub(crate) fn fill_options(rule: peniko::Fill) -> FillOptions {
    match rule {
        peniko::Fill::NonZero => FillOptions::non_zero(),
        peniko::Fill::EvenOdd => FillOptions::even_odd(),
    }
    .with_tolerance(FILL_TOLERANCE_PX)
}

/// Convert kurbo elements to lyon events, every point mapped through `m` (narrowed to f32
/// without repair: a NaN stays NaN so lyon rejects it). Drawing elements before the first
/// `MoveTo` are ignored; an open subpath is ended with `close: false`.
pub(crate) fn push_events(events: &mut Vec<PathEvent>, els: impl Iterator<Item = PathEl>, m: Affine) {
    let p = |q: kurbo::Point| {
        let q = m * q;
        point(narrow_f32(q.x), narrow_f32(q.y))
    };
    // (first, current) of the open subpath.
    let mut open: Option<(Point, Point)> = None;
    for el in els {
        match (el, open) {
            (PathEl::MoveTo(to), _) => {
                if let Some((first, cur)) = open {
                    events.push(PathEvent::End { last: cur, first, close: false });
                }
                let at = p(to);
                events.push(PathEvent::Begin { at });
                open = Some((at, at));
            }
            (PathEl::LineTo(to), Some((first, from))) => {
                let to = p(to);
                events.push(PathEvent::Line { from, to });
                open = Some((first, to));
            }
            (PathEl::QuadTo(c, to), Some((first, from))) => {
                let (ctrl, to) = (p(c), p(to));
                events.push(PathEvent::Quadratic { from, ctrl, to });
                open = Some((first, to));
            }
            (PathEl::CurveTo(c1, c2, to), Some((first, from))) => {
                let (ctrl1, ctrl2, to) = (p(c1), p(c2), p(to));
                events.push(PathEvent::Cubic { from, ctrl1, ctrl2, to });
                open = Some((first, to));
            }
            (PathEl::ClosePath, Some((first, cur))) => {
                events.push(PathEvent::End { last: cur, first, close: true });
                open = None;
            }
            _ => {}
        }
    }
    if let Some((first, cur)) = open {
        events.push(PathEvent::End { last: cur, first, close: false });
    }
}

impl PathTessellator {
    /// Tessellate `els` under `m` with `rule` and an already-premultiplied color into the
    /// solid arenas. On a lyon error the arenas are truncated back to their length before the
    /// op, `tessellation-error` is recorded, and `None` is returned (no command).
    pub(crate) fn fill(
        &mut self,
        plan: &mut FramePlan,
        stats: &mut FrameStats,
        els: impl Iterator<Item = PathEl>,
        m: Affine,
        rule: peniko::Fill,
        color: [f32; 4],
    ) -> Option<Range<u32>> {
        self.events.clear();
        push_events(&mut self.events, els, m);
        self.fill_events(plan, stats, rule, color)
    }

    /// Fill the device-space outline left in `stroke_ctx` by `kurbo::stroke_with` (local
    /// space) mapped through `m`, NonZero.
    pub(crate) fn fill_stroke_outline(
        &mut self,
        plan: &mut FramePlan,
        stats: &mut FrameStats,
        m: Affine,
        color: [f32; 4],
    ) -> Option<Range<u32>> {
        self.events.clear();
        push_events(&mut self.events, self.stroke_ctx.output().elements().iter().copied(), m);
        self.fill_events(plan, stats, peniko::Fill::NonZero, color)
    }

    fn fill_events(
        &mut self,
        plan: &mut FramePlan,
        stats: &mut FrameStats,
        rule: peniko::Fill,
        color: [f32; 4],
    ) -> Option<Range<u32>> {
        let mark = (plan.solid_vertices.len(), plan.solid_indices.len());
        match self.tessellate(rule, color) {
            Ok(()) => Some(self.append(plan)),
            Err(_) => {
                plan.solid_vertices.truncate(mark.0);
                plan.solid_indices.truncate(mark.1);
                stats.record(Unsupported::TessError);
                None
            }
        }
    }

    fn tessellate(&mut self, rule: peniko::Fill, color: [f32; 4]) -> Result<(), TessellationError> {
        self.out.vertices.clear();
        self.out.indices.clear();
        let opts = fill_options(rule);
        self.fill.tessellate(
            self.events.drain(..),
            &opts,
            &mut BuffersBuilder::new(&mut self.out, SolidVertexCtor(color)),
        )
    }

    /// Append `out` to the solid arenas, rebasing indices.
    fn append(&mut self, plan: &mut FramePlan) -> Range<u32> {
        let start = index_u32(plan.solid_indices.len());
        let base = index_u32(plan.solid_vertices.len());
        plan.solid_vertices.extend_from_slice(&self.out.vertices);
        plan.solid_indices.extend(self.out.indices.iter().map(|index| base + index));
        start..index_u32(plan.solid_indices.len())
    }
}

pub(crate) struct SolidVertexCtor(pub(crate) [f32; 4]);
impl FillVertexConstructor<SolidVertex> for SolidVertexCtor {
    fn new_vertex(&mut self, vertex: FillVertex<'_>) -> SolidVertex {
        let p = vertex.position();
        SolidVertex { pos: [p.x, p.y], color: self.0 }
    }
}

#[cfg(test)]
#[path = "tessellate_tests.rs"]
mod tests;
