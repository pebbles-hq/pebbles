//! Rect-clip classification (scissor) and the clip stack.

use kurbo::Affine;
use pebbles_render::paint::display_list::Geometry;

use super::plan::{ClipState, ScissorRect, SdfClip, SdfClipId};

/// A device-space edge within this distance of an integer is "on the pixel grid" (09 §2.2).
pub(crate) const SNAP_EPS: f64 = 1e-3;
/// |b|, |c| ≤ AXIS_EPS × max(|a|, |d|, 1) means "no rotation or skew".
pub(crate) const AXIS_EPS: f64 = 1e-9;
/// |a − d| ≤ UNIFORM_EPS × max(|a|, |d|) means "uniform scale".
pub(crate) const UNIFORM_EPS: f64 = 1e-6;

/// How a clip is realised (09 §2.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ClipKind {
    /// The clip contains the current scissor: nothing changes.
    Noop,
    /// Empty intersection or non-finite input: cull until the matching pop.
    Cull,
    /// The new scissor alone is exact.
    Scissor,
    /// Interim for rows 8–10 until L08/L09: scissor to the device bbox (C17 logs it).
    Bounds,
    /// Scissor (rounded out) + this SDF clip.
    Sdf(SdfClip),
    /// Scissor (rounded out) + a Tier B coverage-mask clip multiplied with the active one (row 8);
    /// the walk builds the record from the geometry.
    Mask,
}

/// The result of classifying a clip geometry.
#[derive(Clone, Copy, Debug)]
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

/// Signed distance from `p` to a rounded rect (negative inside), the WGSL `sd_rrect` on the CPU.
pub(crate) fn sd_rrect(p: kurbo::Point, rect: [f64; 4], radii: [f64; 4]) -> f64 {
    let (cx, cy) = (0.5 * (rect[0] + rect[2]), 0.5 * (rect[1] + rect[3]));
    let (hx, hy) = (0.5 * (rect[2] - rect[0]), 0.5 * (rect[3] - rect[1]));
    let (qx0, qy0) = (p.x - cx, p.y - cy);
    let r_top = if qx0 > 0.0 { radii[1] } else { radii[0] };
    let r_bottom = if qx0 > 0.0 { radii[2] } else { radii[3] };
    let r = if qy0 < 0.0 { r_top } else { r_bottom };
    let (qx, qy) = (qx0.abs() - hx + r, qy0.abs() - hy + r);
    qx.max(qy).min(0.0) + qx.max(0.0).hypot(qy.max(0.0)) - r
}

/// The device rect and radii of an SDF-eligible clip (rows 5–7), else `None`.
fn sdf_of(geom: &Geometry, xf: Affine) -> Option<SdfClip> {
    let [a, b, c, d, ..] = xf.as_coeffs();
    let s = a.abs().max(d.abs()).max(1.0);
    let no_skew = b.abs() <= AXIS_EPS * s && c.abs() <= AXIS_EPS * s;
    let uniform = (a.abs() - d.abs()).abs() <= UNIFORM_EPS * a.abs().max(d.abs());
    let f = |v: f64| super::num::to_f32(v);
    match geom {
        Geometry::RoundedRect(rr) if no_skew && a > 0.0 && d > 0.0 && uniform => {
            let dev = xf.transform_rect_bbox(rr.rect());
            let r = rr.radii();
            let radii = [r.top_left, r.top_right, r.bottom_right, r.bottom_left].map(|v| f(v * a));
            Some(SdfClip {
                rect: [f(dev.x0)?, f(dev.y0)?, f(dev.x1)?, f(dev.y1)?],
                radii: [radii[0]?, radii[1]?, radii[2]?, radii[3]?],
            })
        }
        Geometry::Circle(ci) if no_skew && uniform => {
            let dev = xf.transform_rect_bbox(kurbo::Shape::bounding_box(ci));
            let r = f(ci.radius * a.abs())?;
            Some(SdfClip { rect: [f(dev.x0)?, f(dev.y0)?, f(dev.x1)?, f(dev.y1)?], radii: [r; 4] })
        }
        _ => None,
    }
}

/// Whether the SDF clip contains every pixel of `s` (row 3): all four corners at `sd ≤ −½`.
fn sdf_contains(c: &SdfClip, s: ScissorRect) -> bool {
    let rect = c.rect.map(f64::from);
    let radii = c.radii.map(f64::from);
    [(s.x0, s.y0), (s.x1, s.y0), (s.x0, s.y1), (s.x1, s.y1)]
        .iter()
        .all(|&(x, y)| sd_rrect(kurbo::Point::new(f64::from(x), f64::from(y)), rect, radii) <= -0.5)
}

/// Row 8: a Tier B mask when the clipped area and segment count fit (02 §5.2); rows 9–10 keep
/// the `Bounds` interim until L09 (GPU masks).
fn mask_or_bounds(geom: &Geometry, xf: Affine, scissor: ScissorRect) -> ClipDecision {
    use super::path_strategy::{MASK_MAX_AREA, MASK_MAX_SEGMENTS, segment_count};
    let area = u64::from(scissor.width()) * u64::from(scissor.height());
    if area <= MASK_MAX_AREA && segment_count(geom, xf) <= MASK_MAX_SEGMENTS {
        ClipDecision { kind: ClipKind::Mask, scissor }
    } else {
        ClipDecision { kind: ClipKind::Bounds, scissor }
    }
}

/// Classify a clip geometry under `xf` against the current clip (09 §2.2 rows 1–8; rows 9–10
/// keep the `Bounds` interim until L09).
pub(crate) fn classify_clip(geom: &Geometry, xf: Affine, cur: &ClipState) -> ClipDecision {
    let cull = ClipDecision { kind: ClipKind::Cull, scissor: cur.scissor };
    // Row 1: non-finite.
    if !xf.as_coeffs().iter().all(|v| v.is_finite()) {
        return cull;
    }
    let bbox = xf.transform_rect_bbox(geom.bounding_box());
    if !bbox.is_finite() {
        return cull;
    }
    // Row 2: empty intersection.
    let target = (cur.scissor.x1, cur.scissor.y1);
    let Some(rounded_out) = ScissorRect::from_device_bounds(bbox.x0, bbox.y0, bbox.x1, bbox.y1, target)
        .and_then(|r| r.intersect(cur.scissor))
    else {
        return cull;
    };
    match clip_class(geom, xf) {
        ClipClass::Scissor { device, .. } => {
            // Row 3: contains the scissor.
            let s = cur.scissor;
            if device.x0 <= f64::from(s.x0)
                && device.y0 <= f64::from(s.y0)
                && device.x1 >= f64::from(s.x1)
                && device.y1 >= f64::from(s.y1)
            {
                return ClipDecision { kind: ClipKind::Noop, scissor: cur.scissor };
            }
            let on_grid = |v: f64| (v - v.round()).abs() <= SNAP_EPS;
            if on_grid(device.x0) && on_grid(device.y0) && on_grid(device.x1) && on_grid(device.y1) {
                // Row 4: integer edges → scissor (rounded to nearest).
                let (x0, y0, x1, y1) =
                    (device.x0.round(), device.y0.round(), device.x1.round(), device.y1.round());
                return match ScissorRect::from_device_bounds(x0, y0, x1, y1, target)
                    .and_then(|r| r.intersect(cur.scissor))
                {
                    Some(scissor) => ClipDecision { kind: ClipKind::Scissor, scissor },
                    None => cull,
                };
            }
            // Row 5: a fractional edge → an SDF rect (when the slot is free).
            let f = |v: f64| super::num::to_f32(v).unwrap_or(0.0);
            if cur.sdf == SdfClipId::NONE {
                let c = SdfClip {
                    rect: [f(device.x0), f(device.y0), f(device.x1), f(device.y1)],
                    radii: [0.0; 4],
                };
                return ClipDecision { kind: ClipKind::Sdf(c), scissor: rounded_out };
            }
            mask_or_bounds(geom, xf, rounded_out)
        }
        ClipClass::RoundedRect | ClipClass::Other => {
            // Rows 6–7: rounded rect / circle under a uniform, unrotated transform.
            if let Some(c) = sdf_of(geom, xf) {
                if sdf_contains(&c, cur.scissor) {
                    return ClipDecision { kind: ClipKind::Noop, scissor: cur.scissor };
                }
                if cur.sdf == SdfClipId::NONE {
                    return ClipDecision { kind: ClipKind::Sdf(c), scissor: rounded_out };
                }
            }
            mask_or_bounds(geom, xf, rounded_out)
        }
    }
}

/// The clip stack: the current `ClipState` plus the saved state at each `PushLayer`. An
/// unmatched `PopLayer` is ignored (counted by the caller).
#[derive(Default)]
pub(crate) struct ClipStack {
    stack: Vec<ClipState>,
    current: ClipState,
}

impl ClipStack {
    /// Reset for a new frame (full-target scissor, no SDF or mask clip).
    pub(crate) fn reset(&mut self, target: (u32, u32)) {
        self.stack.clear();
        self.current = ClipState::full(target);
    }

    /// Open entries (tests: the stack is balanced after a walk).
    #[cfg(test)]
    pub(crate) fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Pushes still open (auto-closed at frame end, 09 §2.7).
    pub(crate) fn open(&self) -> usize {
        self.stack.len()
    }

    /// The current clip state (only valid when not culled).
    pub(crate) fn current(&self) -> ClipState {
        self.current
    }

    /// The current scissor is empty — every draw in the subtree is culled.
    pub(crate) fn is_culled(&self) -> bool {
        let s = self.current.scissor;
        s.x1 <= s.x0 || s.y1 <= s.y0
    }

    /// Apply a classified clip (09 §2.2 "Applying a decision"); an SDF clip is appended to
    /// `sdf_clips`. A `Cull` decision empties the scissor so the subtree is culled.
    pub(crate) fn push(&mut self, decision: ClipDecision, sdf_clips: &mut Vec<SdfClip>) {
        self.stack.push(self.current);
        match decision.kind {
            ClipKind::Cull => self.current.scissor = ScissorRect { x0: 0, y0: 0, x1: 0, y1: 0 },
            ClipKind::Noop => {}
            ClipKind::Scissor | ClipKind::Bounds => self.current.scissor = decision.scissor,
            // The walk pushes mask clips through `push_mask` (it builds the record).
            ClipKind::Mask => self.current.scissor = decision.scissor,
            ClipKind::Sdf(c) => {
                sdf_clips.push(c);
                self.current.scissor = decision.scissor;
                self.current.sdf = SdfClipId(u32::try_from(sdf_clips.len() - 1).unwrap_or(0));
            }
        }
    }

    /// Push a mask clip whose record the walk appended (09 L08).
    pub(crate) fn push_mask(&mut self, scissor: ScissorRect, id: super::plan::MaskClipId) {
        self.stack.push(self.current);
        self.current.scissor = scissor;
        self.current.mask = id;
    }

    /// Pop the matching clip; returns false for an unmatched pop.
    pub(crate) fn pop(&mut self) -> bool {
        match self.stack.pop() {
            Some(prev) => {
                self.current = prev;
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Rect;

    fn cur(size: (u32, u32)) -> ClipState {
        ClipState::full(size)
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
        // Rotated into x ∈ [−50, 0] it would be entirely off-target (a cull); move it into view.
        let m = Affine::translate((100.0, 0.0)) * Affine::rotate(std::f64::consts::FRAC_PI_2);
        let d = classify_clip(&geom, m, &cur((200, 200)));
        assert_eq!(d.kind, ClipKind::Scissor);
        assert_eq!(d.scissor, ScissorRect { x0: 50, y0: 0, x1: 100, y1: 100 });
    }

    #[test]
    fn rotated_30_rect_is_mask_clip() {
        let geom = Geometry::Rect(Rect::new(0.0, 0.0, 100.0, 50.0));
        let d = classify_clip(&geom, Affine::rotate(std::f64::consts::FRAC_PI_6), &cur((200, 200)));
        assert_eq!(d.kind, ClipKind::Mask);
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

    // ---- L03 (09 §2.2 rows 1–7) ----

    fn decide(g: Geometry, m: Affine) -> ClipDecision {
        classify_clip(&g, m, &cur((100, 100)))
    }

    #[test]
    fn integer_rect_is_scissor() {
        let d = decide(Geometry::Rect(Rect::new(10.0, 10.0, 50.0, 40.0)), Affine::IDENTITY);
        assert_eq!(d.kind, ClipKind::Scissor);
        assert_eq!(d.scissor, ScissorRect { x0: 10, y0: 10, x1: 50, y1: 40 });
    }

    #[test]
    fn rect_rotated_90_is_scissor() {
        let m = Affine::translate((60.0, 0.0)) * Affine::rotate(std::f64::consts::FRAC_PI_2);
        assert_eq!(decide(Geometry::Rect(Rect::new(10.0, 10.0, 50.0, 40.0)), m).kind, ClipKind::Scissor);
    }

    #[test]
    fn fractional_rect_is_sdf() {
        let d = decide(Geometry::Rect(Rect::new(10.5, 10.0, 50.0, 40.0)), Affine::IDENTITY);
        let ClipKind::Sdf(c) = d.kind else { panic!("{:?}", d.kind) };
        assert_eq!(c.radii, [0.0; 4]);
        assert_eq!(d.scissor.x0, 10);
    }

    #[test]
    fn rrect_uniform_scale_is_sdf_with_scaled_radii() {
        let g = Geometry::RoundedRect(kurbo::RoundedRect::new(0.0, 0.0, 40.0, 40.0, 8.0));
        let ClipKind::Sdf(c) = decide(g, Affine::scale(2.0)).kind else { panic!() };
        assert_eq!(c.radii, [16.0; 4]);
    }

    #[test]
    fn rrect_non_uniform_scale_is_mask() {
        let g = Geometry::RoundedRect(kurbo::RoundedRect::new(0.0, 0.0, 40.0, 40.0, 8.0));
        assert_eq!(decide(g, Affine::scale_non_uniform(2.0, 1.0)).kind, ClipKind::Mask);
    }

    #[test]
    fn circle_under_rotation_is_sdf() {
        let g = || Geometry::Circle(kurbo::Circle::new((50.0, 50.0), 20.0));
        assert!(matches!(decide(g(), Affine::IDENTITY).kind, ClipKind::Sdf(_)));
        let rot = Affine::rotate_about(std::f64::consts::FRAC_PI_6, kurbo::Point::new(50.0, 50.0));
        assert_eq!(decide(g(), rot).kind, ClipKind::Mask, "a rotated circle is a mask (row 8)");
    }

    #[test]
    fn second_rrect_with_sdf_taken_is_mask() {
        let mut c = cur((100, 100));
        c.sdf = SdfClipId(1);
        let g = Geometry::RoundedRect(kurbo::RoundedRect::new(10.0, 10.0, 60.0, 60.0, 8.0));
        assert_eq!(classify_clip(&g, Affine::IDENTITY, &c).kind, ClipKind::Mask);
    }

    #[test]
    fn containing_rect_is_noop() {
        assert_eq!(
            decide(Geometry::Rect(Rect::new(-5.0, -5.0, 200.0, 200.0)), Affine::IDENTITY).kind,
            ClipKind::Noop
        );
    }

    #[test]
    fn rrect_containing_scissor_is_noop() {
        let g = Geometry::RoundedRect(kurbo::RoundedRect::new(-20.0, -20.0, 120.0, 120.0, 8.0));
        assert_eq!(decide(g, Affine::IDENTITY).kind, ClipKind::Noop);
    }

    #[test]
    fn nan_transform_culls() {
        let m = Affine::new([f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0]);
        assert_eq!(decide(Geometry::Rect(Rect::new(0.0, 0.0, 10.0, 10.0)), m).kind, ClipKind::Cull);
    }

    #[test]
    fn disjoint_clip_culls_until_pop() {
        use pebbles_render::paint::{Painter, Scene};
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        p.push_layer(
            peniko::Fill::NonZero,
            peniko::BlendMode::default(),
            1.0,
            Affine::IDENTITY,
            &Rect::new(200.0, 200.0, 300.0, 300.0),
        );
        p.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            peniko::Color::BLACK,
            None,
            &Rect::new(0.0, 0.0, 50.0, 50.0),
        );
        p.pop_layer();
        p.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            peniko::Color::BLACK,
            None,
            &Rect::new(0.0, 0.0, 50.0, 50.0),
        );
        let (plan, _) = crate::prepare::test_util::prepare_scene(&s, 100, 100);
        assert_eq!(plan.cmds.len(), 1, "only the fill after the pop");
        assert_eq!(plan.stats.clips_culled, 1);
    }

    #[test]
    fn unbalanced_pop_is_ignored() {
        use pebbles_render::paint::{Painter, Scene};
        let mut s = Scene::new();
        Painter::new(&mut s).pop_layer();
        let (plan, _) = crate::prepare::test_util::prepare_scene(&s, 100, 100);
        assert_eq!(plan.stats.unbalanced_pops, 1);
    }

    #[test]
    fn open_push_is_autoclosed() {
        use pebbles_render::paint::{Painter, Scene};
        let mut s = Scene::new();
        Painter::new(&mut s).push_layer(
            peniko::Fill::NonZero,
            peniko::BlendMode::default(),
            1.0,
            Affine::IDENTITY,
            &Rect::new(10.0, 10.0, 50.0, 50.0),
        );
        let (plan, _) = crate::prepare::test_util::prepare_scene(&s, 100, 100);
        assert_eq!(plan.stats.autoclosed, 1);
    }

    // ---- L04 ----

    #[test]
    fn rrect_push_assigns_sdf_record() {
        use pebbles_render::paint::{Painter, Scene};
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        let rr = kurbo::RoundedRect::new(10.0, 10.0, 60.0, 60.0, 6.0);
        p.push_layer(peniko::Fill::NonZero, peniko::BlendMode::default(), 1.0, Affine::IDENTITY, &rr);
        p.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            peniko::Color::BLACK,
            None,
            &Rect::new(0.0, 0.0, 100.0, 100.0),
        );
        p.pop_layer();
        p.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            peniko::Color::BLACK,
            None,
            &Rect::new(0.0, 0.0, 100.0, 100.0),
        );
        let (plan, _) = crate::prepare::test_util::prepare_scene(&s, 100, 100);
        let clips: Vec<_> = plan
            .cmds
            .iter()
            .map(|c| match c {
                crate::prepare::DrawCmd::Shapes { clip, .. }
                | crate::prepare::DrawCmd::Solid { clip, .. } => clip.sdf,
                _ => SdfClipId::NONE,
            })
            .collect();
        assert_eq!(clips, vec![SdfClipId(1), SdfClipId::NONE], "AC1 + AC2 (restored on pop)");
        assert_eq!(plan.sdf_clips[1].radii, [6.0; 4]);
    }

    #[test]
    fn sdf_record_restored_on_pop() {
        let mut stack = ClipStack::default();
        stack.reset((100, 100));
        let mut records = vec![SdfClip::NONE];
        let d = classify_clip(
            &Geometry::Circle(kurbo::Circle::new((50.0, 50.0), 20.0)),
            Affine::IDENTITY,
            &stack.current(),
        );
        stack.push(d, &mut records);
        assert_eq!(stack.current().sdf, SdfClipId(1));
        assert!(stack.pop());
        assert_eq!(stack.current().sdf, SdfClipId::NONE);
    }

    // ---- L08 (row 8: Tier B mask clips) ----

    fn star(r: f64) -> Geometry {
        let mut p = kurbo::BezPath::new();
        for i in 0..5 {
            let a = -std::f64::consts::FRAC_PI_2 + f64::from(i) * 4.0 * std::f64::consts::PI / 5.0;
            let pt = (50.0 + r * a.cos(), 50.0 + r * a.sin());
            if i == 0 { p.move_to(pt) } else { p.line_to(pt) }
        }
        p.close_path();
        Geometry::Path(p)
    }

    #[test]
    fn path_clip_is_mask_when_small() {
        assert_eq!(decide(star(40.0), Affine::IDENTITY).kind, ClipKind::Mask);
    }

    #[test]
    fn huge_path_clip_is_not_a_cpu_mask() {
        // 1000×1000 target: the clipped bbox exceeds MASK_MAX_AREA → rows 9–10 (Bounds until L09).
        let d = classify_clip(&star(400.0), Affine::translate((450.0, 450.0)), &cur((1000, 1000)));
        assert_ne!(d.kind, ClipKind::Mask);
    }

    #[test]
    fn nested_masks_multiply() {
        use pebbles_render::paint::{Painter, Scene};
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        let Geometry::Path(a) = star(45.0) else { unreachable!() };
        p.push_layer(peniko::Fill::NonZero, peniko::BlendMode::default(), 1.0, Affine::IDENTITY, &a);
        let e = kurbo::Ellipse::new((50.0, 55.0), (40.0, 25.0), 0.3);
        p.push_layer(peniko::Fill::NonZero, peniko::BlendMode::default(), 1.0, Affine::IDENTITY, &e);
        p.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            peniko::Color::BLACK,
            None,
            &Rect::new(0.0, 0.0, 100.0, 100.0),
        );
        p.pop_layer();
        p.pop_layer();
        let (plan, _) = crate::prepare::test_util::prepare_scene(&s, 100, 100);
        assert_eq!(plan.mask_clips.len(), 3, "NONE + outer + nested");
        assert_eq!(plan.mask_clips[2].sources.len(), 2, "the nested record carries both clips (multiplied)");
        assert_ne!(plan.mask_clips[1].key, plan.mask_clips[2].key);
        assert_eq!(plan.stats.clips_mask, 2);
    }
}
