//! Rect-clip classification (scissor) and the clip stack.

use kurbo::Affine;
use pebbles_render::paint::display_list::Geometry;

use super::plan::{ClipState, ScissorRect};

/// How a clip is realised. Phase C produces only these four; L01 deletes `Bounds`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClipKind {
    /// The clip contains the current scissor: nothing changes.
    Noop,
    /// Empty intersection or non-finite input: cull until the matching pop.
    Cull,
    /// Axis-aligned rect after transform (incl. zero-radius rounded rects): scissor alone is exact.
    Scissor,
    /// Any other clip → scissor to its device bounding box (C17 logs it).
    Bounds,
}

/// The result of classifying a clip geometry.
pub(crate) struct ClipDecision {
    pub kind: ClipKind,
    /// The scissor to apply (`cur.scissor` for `Noop`/`Cull`).
    pub scissor: ScissorRect,
}

/// Classify a clip geometry under `xf` against the current clip (09 §2.2, Phase-C rows only).
pub(crate) fn classify_clip(geom: &Geometry, xf: Affine, cur: &ClipState) -> ClipDecision {
    let c = xf.as_coeffs();
    // A rect (or zero-radius rounded rect) under an axis-preserving transform → scissor.
    let rect = match geom {
        Geometry::Rect(r) => Some(*r),
        Geometry::RoundedRect(rr) => {
            let radii = rr.radii();
            let zero = radii.top_left.abs() <= 1e-9
                && radii.top_right.abs() <= 1e-9
                && radii.bottom_right.abs() <= 1e-9
                && radii.bottom_left.abs() <= 1e-9;
            zero.then_some(rr.rect())
        }
        _ => None,
    };
    let axis_preserving =
        (c[1].abs() < 1e-9 && c[2].abs() < 1e-9) || (c[0].abs() < 1e-9 && c[3].abs() < 1e-9);
    let bbox = |geom: &Geometry| xf.transform_rect_bbox(geom.bounding_box());
    if let Some(rect) = rect {
        if axis_preserving {
            let b = xf.transform_rect_bbox(rect);
            return finish(b, cur, ClipKind::Scissor);
        }
        // A rotated rect clip → bounds (bbox scissor).
        return finish(bbox(geom), cur, ClipKind::Bounds);
    }
    // Circles, paths, and any non-finite geometry → bounds.
    if !c.iter().all(|v| v.is_finite()) {
        return ClipDecision { kind: ClipKind::Cull, scissor: cur.scissor };
    }
    finish(bbox(geom), cur, ClipKind::Bounds)
}

fn finish(b: kurbo::Rect, cur: &ClipState, kind: ClipKind) -> ClipDecision {
    let Some(scissor) =
        ScissorRect::from_device_bounds(b.x0, b.y0, b.x1, b.y1, (cur.scissor.x1, cur.scissor.y1))
    else {
        return ClipDecision { kind: ClipKind::Cull, scissor: cur.scissor };
    };
    match scissor.intersect(cur.scissor) {
        None => ClipDecision { kind: ClipKind::Cull, scissor: cur.scissor },
        Some(intersected) if intersected == cur.scissor => {
            ClipDecision { kind: ClipKind::Noop, scissor: cur.scissor }
        }
        Some(intersected) => ClipDecision { kind, scissor: intersected },
    }
}

/// The clip stack: the current (intersected) scissor plus the saved scissor at each
/// `PushLayer`. An unmatched `PopLayer` is ignored.
#[derive(Default)]
pub(crate) struct ClipStack {
    stack: Vec<ScissorRect>,
    current: ScissorRect,
}

impl ClipStack {
    /// Reset for a new frame (full-target scissor).
    pub(crate) fn reset(&mut self, target: (u32, u32)) {
        self.stack.clear();
        self.current = ScissorRect::full(target);
    }

    /// The current clip state (only valid when not culled).
    pub(crate) fn current(&self) -> ClipState {
        ClipState { scissor: self.current }
    }

    /// The current scissor is empty — every draw in the subtree is culled.
    pub(crate) fn is_culled(&self) -> bool {
        self.current.x1 <= self.current.x0 || self.current.y1 <= self.current.y0
    }

    /// Apply a classified clip, intersecting with the parent. A `Cull` decision empties the
    /// current scissor so the whole subtree is culled until the matching pop.
    pub(crate) fn push(&mut self, decision: ClipDecision) {
        self.stack.push(self.current);
        self.current = match decision.kind {
            ClipKind::Cull => ScissorRect { x0: 0, y0: 0, x1: 0, y1: 0 },
            _ => decision.scissor,
        };
    }

    /// Pop the matching clip; an unmatched pop is ignored.
    pub(crate) fn pop(&mut self) {
        if let Some(prev) = self.stack.pop() {
            self.current = prev;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Rect;

    fn cur(size: (u32, u32)) -> ClipState {
        ClipState { scissor: ScissorRect::full(size) }
    }

    #[test]
    fn negative_origin_clamps_to_zero() {
        let s = ScissorRect::from_device_bounds(-50.0, -10.0, 100.0, 40.0, (200, 100)).unwrap();
        assert_eq!(s, ScissorRect { x0: 0, y0: 0, x1: 100, y1: 40 });
    }

    #[test]
    fn fully_offscreen_clip_culls_subtree() {
        let geom = Geometry::Rect(Rect::new(300.0, 0.0, 400.0, 100.0));
        let d = classify_clip(&geom, Affine::IDENTITY, &cur((100, 100)));
        assert_eq!(d.kind, ClipKind::Cull);
    }

    #[test]
    fn nan_bounds_cull() {
        assert!(ScissorRect::from_device_bounds(f64::NAN, 0.0, 100.0, 100.0, (100, 100)).is_none());
    }

    #[test]
    fn rotated_90_rect_is_scissor_clip() {
        let geom = Geometry::Rect(Rect::new(0.0, 0.0, 100.0, 50.0));
        let d = classify_clip(&geom, Affine::rotate(std::f64::consts::FRAC_PI_2), &cur((200, 200)));
        assert_eq!(d.kind, ClipKind::Scissor);
    }

    #[test]
    fn rotated_30_rect_is_bounds_clip() {
        let geom = Geometry::Rect(Rect::new(0.0, 0.0, 100.0, 50.0));
        let d = classify_clip(&geom, Affine::rotate(std::f64::consts::FRAC_PI_6), &cur((200, 200)));
        assert_eq!(d.kind, ClipKind::Bounds);
    }

    #[test]
    fn zero_radius_rrect_is_scissor_clip() {
        let rr = kurbo::RoundedRect::from_rect(Rect::new(0.0, 0.0, 100.0, 50.0), 0.0);
        let d = classify_clip(&Geometry::RoundedRect(rr), Affine::IDENTITY, &cur((200, 200)));
        assert_eq!(d.kind, ClipKind::Scissor);
    }

    #[test]
    fn huge_rect_intersection_does_not_overflow() {
        let geom = Geometry::Rect(Rect::new(10.0, 10.0, 1e12, 1e12));
        let d = classify_clip(&geom, Affine::IDENTITY, &cur((100, 100)));
        assert_eq!(d.scissor, ScissorRect { x0: 10, y0: 10, x1: 100, y1: 100 });
    }
}
