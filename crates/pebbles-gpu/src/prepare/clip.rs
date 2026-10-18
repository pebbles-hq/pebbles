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

/// The shape class of a clip under its device transform (R07).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ClipClass {
    /// An axis-aligned device rectangle: a scissor alone is exact. `aligned` = every edge is
    /// within 1/256 px of an integer (no partial-pixel coverage at the edges).
    Scissor { device: kurbo::Rect, aligned: bool },
    /// A rounded rect with a radius > 0, or a circle (09).
    RoundedRect,
    /// Anything else (rotated rects, general paths).
    Other,
}

/// `|b|,|c| ≤ 1e-9·max(|a|,|d|)` — scale/mirror only.
fn axis_aligned(m: Affine) -> bool {
    let [a, b, c, d, ..] = m.as_coeffs();
    let s = a.abs().max(d.abs());
    b.abs() <= 1e-9 * s && c.abs() <= 1e-9 * s
}

/// `|a|,|d| ≤ 1e-9·max(|b|,|c|)` — a 90°/270° turn, mirrored or not.
fn quarter_turn(m: Affine) -> bool {
    let [a, b, c, d, ..] = m.as_coeffs();
    let s = b.abs().max(c.abs());
    a.abs() <= 1e-9 * s && d.abs() <= 1e-9 * s
}

/// The legacy four-point rectangle path (`MoveTo, LineTo ×3[+1 back to start], ClosePath`) whose
/// device edges are all horizontal or vertical.
fn rect_path(p: &kurbo::BezPath, m: Affine) -> Option<kurbo::Rect> {
    use kurbo::PathEl::{ClosePath, LineTo, MoveTo};
    let pts = match p.elements() {
        [MoveTo(a), LineTo(b), LineTo(c), LineTo(d), ClosePath] => [*a, *b, *c, *d],
        [MoveTo(a), LineTo(b), LineTo(c), LineTo(d), LineTo(e), ClosePath] if e == a => [*a, *b, *c, *d],
        _ => return None,
    };
    let dev = pts.map(|q| m * q);
    let straight = dev
        .iter()
        .zip(dev.iter().cycle().skip(1))
        .all(|(p, q)| (p.x - q.x).abs() <= 1e-3 || (p.y - q.y).abs() <= 1e-3);
    let [first, rest @ ..] = dev;
    straight.then(|| rest.iter().fold(kurbo::Rect::from_points(first, first), |r, q| r.union_pt(*q)))
}

/// Classify a clip shape under `m` (02 §6.5, R07).
pub(crate) fn clip_class(g: &Geometry, m: Affine) -> ClipClass {
    let scissor = |device: kurbo::Rect| {
        let on_px = |v: f64| (v - v.round()).abs() <= 1.0 / 256.0;
        let aligned = on_px(device.x0) && on_px(device.y0) && on_px(device.x1) && on_px(device.y1);
        ClipClass::Scissor { device, aligned }
    };
    let preserving = axis_aligned(m) || quarter_turn(m);
    match g {
        Geometry::Rect(r) if preserving => scissor(m.transform_rect_bbox(*r)),
        Geometry::RoundedRect(rr) => {
            let r = rr.radii();
            let zero =
                [r.top_left, r.top_right, r.bottom_right, r.bottom_left].iter().all(|v| v.abs() < 1e-9);
            match (zero, preserving) {
                (true, true) => scissor(m.transform_rect_bbox(rr.rect())),
                (true, false) => ClipClass::Other,
                (false, _) => ClipClass::RoundedRect,
            }
        }
        Geometry::Circle(_) => ClipClass::RoundedRect,
        Geometry::Path(p) => rect_path(p, m).map_or(ClipClass::Other, scissor),
        _ => ClipClass::Other,
    }
}

/// Classify a clip geometry under `xf` against the current clip (09 §2.2, Phase-C rows only):
/// an R07 `Scissor` class goes through C11's snap + intersect; everything else is a bbox
/// scissor (`Bounds`, C17) until L01.
pub(crate) fn classify_clip(geom: &Geometry, xf: Affine, cur: &ClipState) -> ClipDecision {
    if !xf.as_coeffs().iter().all(|v| v.is_finite()) {
        return ClipDecision { kind: ClipKind::Cull, scissor: cur.scissor };
    }
    match clip_class(geom, xf) {
        ClipClass::Scissor { device, .. } => finish(device, cur, ClipKind::Scissor),
        ClipClass::RoundedRect | ClipClass::Other => {
            finish(xf.transform_rect_bbox(geom.bounding_box()), cur, ClipKind::Bounds)
        }
    }
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

    /// Open entries (tests: the stack is balanced after a walk).
    #[cfg(test)]
    pub(crate) fn depth(&self) -> usize {
        self.stack.len()
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

    // ---- R07: clip shape classes ----

    fn class(g: Geometry, m: Affine) -> ClipClass {
        clip_class(&g, m)
    }

    fn is_scissor(c: ClipClass) -> bool {
        matches!(c, ClipClass::Scissor { .. })
    }

    #[test]
    fn rect_identity_is_scissor() {
        let c = class(Geometry::Rect(Rect::new(10.0, 10.0, 50.0, 40.0)), Affine::IDENTITY);
        assert_eq!(c, ClipClass::Scissor { device: Rect::new(10.0, 10.0, 50.0, 40.0), aligned: true });
    }

    #[test]
    fn zero_radius_rrect_is_scissor() {
        // decorated.rs:207's image clip: `rect.to_rounded_rect(0.0)`.
        let rr = Rect::new(0.0, 0.0, 40.0, 40.0).to_rounded_rect(0.0);
        assert!(is_scissor(class(Geometry::RoundedRect(rr), Affine::scale(2.0))));
    }

    #[test]
    fn rrect_radius_4_is_rrect() {
        let rr = Rect::new(0.0, 0.0, 40.0, 40.0).to_rounded_rect(4.0);
        assert_eq!(class(Geometry::RoundedRect(rr), Affine::IDENTITY), ClipClass::RoundedRect);
    }

    #[test]
    fn quarter_turn_rect_is_scissor() {
        let m = Affine::rotate(std::f64::consts::FRAC_PI_2).then_translate((60.0, 0.0).into());
        let ClipClass::Scissor { device, .. } = class(Geometry::Rect(Rect::new(0.0, 0.0, 100.0, 50.0)), m)
        else {
            panic!("not a scissor");
        };
        assert!((device.width() - 50.0).abs() < 1e-6 && (device.height() - 100.0).abs() < 1e-6, "{device:?}");
    }

    #[test]
    fn mirrored_rect_is_scissor() {
        let m = Affine::scale_non_uniform(-1.0, 1.0).then_translate((100.0, 0.0).into());
        assert!(is_scissor(class(Geometry::Rect(Rect::new(10.0, 10.0, 50.0, 40.0)), m)));
    }

    #[test]
    fn rotate_15_degrees_is_other() {
        let m = Affine::rotate(15f64.to_radians());
        assert_eq!(class(Geometry::Rect(Rect::new(10.0, 10.0, 50.0, 40.0)), m), ClipClass::Other);
    }

    #[test]
    fn legacy_four_point_path_is_scissor() {
        let mut p = kurbo::BezPath::new();
        p.move_to((10.0, 10.0));
        p.line_to((50.0, 10.0));
        p.line_to((50.0, 40.0));
        p.line_to((10.0, 40.0));
        p.close_path();
        assert_eq!(
            class(Geometry::Path(p), Affine::IDENTITY),
            ClipClass::Scissor { device: Rect::new(10.0, 10.0, 50.0, 40.0), aligned: true }
        );
    }

    #[test]
    fn triangle_path_is_other() {
        let mut p = kurbo::BezPath::new();
        p.move_to((10.0, 10.0));
        p.line_to((50.0, 10.0));
        p.line_to((30.0, 40.0));
        p.close_path();
        assert_eq!(class(Geometry::Path(p), Affine::IDENTITY), ClipClass::Other);
    }

    #[test]
    fn circle_is_rrect() {
        let c = Geometry::Circle(kurbo::Circle::new((20.0, 20.0), 10.0));
        assert_eq!(class(c, Affine::IDENTITY), ClipClass::RoundedRect);
    }

    #[test]
    fn fractional_rect_is_unaligned() {
        let c = class(Geometry::Rect(Rect::new(10.5, 10.0, 50.0, 40.0)), Affine::IDENTITY);
        assert!(matches!(c, ClipClass::Scissor { aligned: false, .. }), "{c:?}");
    }
}
