//! Tier A primitives as exact kurbo geometry (05 §6.7a) with 02 §5.3 pixel snapping.

use kurbo::{Affine, BezPath, Rect, RoundedRect, Shape, Stroke};
use pebbles_render::paint::display_list::Geometry;
use peniko::Fill;

use super::coverage::{FLATTEN_TOL_PX, fill_coverage};
use super::stroke::{device_width, stroke_coverage};

/// Whether an op is snapped to the pixel grid (02 §5.3; `PixelSnap` in the display list, R18).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Snap {
    Auto,
    Off,
}

/// `round_half_up`.
fn round_half_up(v: f64) -> f64 {
    (v + 0.5).floor()
}

/// Snap one axis `[a, b]`: each edge to the nearest integer; never collapse (a 1-px box at
/// `floor(min)`).
fn snap_axis(a: f64, b: f64) -> (f64, f64) {
    let (lo, hi) = (a.min(b), a.max(b));
    let (s0, s1) = (round_half_up(lo), round_half_up(hi));
    if s0 == s1 { (lo.floor(), lo.floor() + 1.0) } else { (s0, s1) }
}

/// A device rect with every edge snapped (02 §5.3).
pub fn snap_rect(r: Rect) -> Rect {
    let (x0, x1) = snap_axis(r.x0, r.x1);
    let (y0, y1) = snap_axis(r.y0, r.y1);
    Rect::new(x0, y0, x1, y1)
}

/// Axis-aligned: no rotation or skew (90° multiples allowed).
pub fn axis_aligned(t: Affine) -> bool {
    let [a, b, c, d, _, _] = t.as_coeffs();
    (b == 0.0 && c == 0.0) || (a == 0.0 && d == 0.0)
}

/// The device-space rounded rect of a Tier A geometry under an axis-aligned `t`, if the
/// geometry is a rect or a rounded rect (circles keep exact geometry: §5.3 snaps rect edges).
fn device_rounded(g: &Geometry, t: Affine) -> Option<RoundedRect> {
    let (rect, radii) = match g {
        Geometry::Rect(r) => (*r, None),
        Geometry::RoundedRect(rr) => (rr.rect(), Some(rr.radii())),
        _ => return None,
    };
    let b = t.transform_rect_bbox(rect);
    let s = t.determinant().abs().sqrt();
    let radii = radii.map_or(kurbo::RoundedRectRadii::from_single_radius(0.0), |r| {
        kurbo::RoundedRectRadii::new(r.top_left * s, r.top_right * s, r.bottom_right * s, r.bottom_left * s)
    });
    // A quarter turn permutes the corners; uniform radii (the common case) are unaffected.
    Some(RoundedRect::from_rect(b, radii))
}

/// The op's own path (`Geometry::to_path`): dashes start where the recorded geometry starts.
fn shape_path(g: &Geometry, t: Affine) -> BezPath {
    let tol = FLATTEN_TOL_PX / t.spectral_norm().max(1e-12);
    g.to_path(tol).into_owned()
}

/// Fill coverage of a display-list geometry. Rects and rounded rects under an axis-aligned
/// transform with `Snap::Auto` are snapped in device space first.
pub fn fill_geometry(g: &Geometry, t: Affine, rule: Fill, snap: Snap, w: u32, h: u32) -> Vec<f64> {
    if snap == Snap::Auto
        && axis_aligned(t)
        && let Some(rr) = device_rounded(g, t)
    {
        let snapped = RoundedRect::from_rect(snap_rect(rr.rect()), rr.radii());
        return fill_coverage(&snapped.to_path(FLATTEN_TOL_PX), Affine::IDENTITY, rule, w, h);
    }
    fill_coverage(&shape_path(g, t), t, rule, w, h)
}

/// Stroke coverage of a display-list geometry. Under `Snap::Auto` and an axis-aligned
/// transform, a rect / rounded-rect border of device width `wd ≥ 1` becomes `round(wd)` px
/// whose outer edge is snapped, so it covers whole pixels; thinner borders use the hairline
/// rule.
pub fn stroke_geometry(g: &Geometry, style: &Stroke, t: Affine, snap: Snap, w: u32, h: u32) -> Vec<f64> {
    let wd = device_width(style.width, t);
    let dashed = !style.dash_pattern.is_empty();
    if snap == Snap::Auto
        && axis_aligned(t)
        && wd >= 1.0
        && !dashed
        && let Some(rr) = device_rounded(g, t)
    {
        // The border is the ring between the snapped outer box and the box inset by the
        // rounded width (corner radii grow / shrink by half the width about the center line).
        let width = round_half_up(wd);
        let outer = snap_rect(rr.rect().inflate(wd / 2.0, wd / 2.0));
        let inner = outer.inset(-width);
        let r = rr.radii();
        let grow = |d: f64| {
            let f = |v: f64| if v > 0.0 { (v + d).max(0.0) } else { 0.0 };
            kurbo::RoundedRectRadii::new(f(r.top_left), f(r.top_right), f(r.bottom_right), f(r.bottom_left))
        };
        let mut ring = RoundedRect::from_rect(outer, grow(width / 2.0)).to_path(FLATTEN_TOL_PX);
        if inner.width() > 0.0 && inner.height() > 0.0 {
            ring.extend(RoundedRect::from_rect(inner, grow(-width / 2.0)).to_path(FLATTEN_TOL_PX));
        }
        return fill_coverage(&ring, Affine::IDENTITY, Fill::EvenOdd, w, h);
    }
    stroke_coverage(&shape_path(g, t), style, t, w, h)
}
