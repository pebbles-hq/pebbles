//! Rect-clip classification (scissor) and the clip stack.

use kurbo::Affine;

use super::plan::{ClipState, ScissorRect};

/// The full-target scissor.
pub(crate) fn full(target: (u32, u32)) -> ScissorRect {
    ScissorRect { x0: 0, y0: 0, x1: target.0, y1: target.1 }
}

/// Corner max/min intersection. Empty (non-covering) when `x1 <= x0 || y1 <= y0`.
pub(crate) fn intersect(a: ScissorRect, b: ScissorRect) -> ScissorRect {
    ScissorRect { x0: a.x0.max(b.x0), y0: a.y0.max(b.y0), x1: a.x1.min(b.x1), y1: a.y1.min(b.y1) }
}

/// The clip stack: the current (intersected) scissor plus the saved scissor at each
/// `PushLayer`. An unmatched `PopLayer` is ignored (the current clip is unchanged).
#[derive(Default)]
pub(crate) struct ClipStack {
    stack: Vec<ScissorRect>,
    current: ScissorRect,
}

impl ClipStack {
    /// Reset for a new frame (full-target scissor).
    pub(crate) fn reset(&mut self, target: (u32, u32)) {
        self.stack.clear();
        self.current = full(target);
    }

    /// The current clip state.
    pub(crate) fn current(&self) -> ClipState {
        ClipState { scissor: self.current }
    }

    /// The current scissor is empty — every draw in the subtree is culled.
    pub(crate) fn is_culled(&self) -> bool {
        self.current.x1 <= self.current.x0 || self.current.y1 <= self.current.y0
    }

    /// Push a clip; `None` means "no rect scissor" (keep the current clip).
    pub(crate) fn push(&mut self, scissor: Option<ScissorRect>) {
        self.stack.push(self.current);
        if let Some(s) = scissor {
            self.current = intersect(self.current, s);
        }
    }

    /// Pop the matching clip; an unmatched pop is ignored.
    pub(crate) fn pop(&mut self) {
        if let Some(prev) = self.stack.pop() {
            self.current = prev;
        }
    }
}

/// If `path` (a recorded rect clip) maps to another axis-aligned rectangle under
/// `transform` (translate + scale, no rotation/shear), return its physical-pixel
/// scissor rect; otherwise `None` (non-rect clips are Phase 4's job).
pub(crate) fn scissor_rect(path: &kurbo::BezPath, transform: Affine) -> Option<ScissorRect> {
    let elements = path.elements();
    if elements.len() != 5 {
        return None;
    }
    let corner = |el: &kurbo::PathEl| -> Option<kurbo::Point> {
        match *el {
            kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) => Some(p),
            _ => None,
        }
    };
    let p0 = corner(&elements[0])?;
    let p1 = corner(&elements[1])?;
    let p2 = corner(&elements[2])?;
    let p3 = corner(&elements[3])?;
    if !matches!(elements[4], kurbo::PathEl::ClosePath) {
        return None;
    }
    let a = transform * p0;
    let b = transform * p1;
    let c = transform * p2;
    let d = transform * p3;
    const EPS: f64 = 1e-3;
    if (a.y - b.y).abs() > EPS
        || (c.y - d.y).abs() > EPS
        || (b.x - c.x).abs() > EPS
        || (d.x - a.x).abs() > EPS
    {
        return None;
    }
    let min_x = a.x.min(b.x).min(c.x).min(d.x);
    let max_x = a.x.max(b.x).max(c.x).max(d.x);
    let min_y = a.y.min(b.y).min(c.y).min(d.y);
    let max_y = a.y.max(b.y).max(c.y).max(d.y);
    Some(ScissorRect {
        x0: min_x.floor().max(0.0) as u32,
        y0: min_y.floor().max(0.0) as u32,
        x1: (max_x.ceil().max(0.0)) as u32,
        y1: (max_y.ceil().max(0.0)) as u32,
    })
}
