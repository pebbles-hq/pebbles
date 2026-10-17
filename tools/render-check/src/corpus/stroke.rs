//! `stroke/` — strokes (§7.2) — Slice P1 — 128×128 — all with scale variants.

use pebbles_render::paint::{Painter, kurbo, peniko};

use kurbo::{Affine, BezPath, Cap, Circle, Join, Line, Rect, RoundedRect, Shape, Stroke};
use peniko::Color;

use crate::case::{Case, Category::Geometry, Slice::P1, ops, with_scale_variants};

const INK: Color = Color::from_rgba8(0x11, 0x18, 0x27, 0xFF);

// §7.2 marks widths, under_nonuniform_scale, under_rotation and hairline_grid
// `xfail until C10`. C10 is done (bcd5a6f), so per §2.6 the markers are gone: they must pass.

/// Every `stroke/` case.
pub fn cases() -> Vec<Case> {
    let base = [
        ops("stroke/widths", Geometry, P1, 128, 128, widths),
        ops("stroke/joins", Geometry, P1, 128, 128, joins),
        ops("stroke/caps", Geometry, P1, 128, 128, caps),
        ops("stroke/dashes", Geometry, P1, 128, 128, dashes),
        ops("stroke/miter_limit", Geometry, P1, 128, 128, miter_limit),
        ops("stroke/curve", Geometry, P1, 128, 128, curve),
        ops("stroke/closed_rrect_border", Geometry, P1, 128, 128, closed_rrect_border),
        ops("stroke/under_nonuniform_scale", Geometry, P1, 128, 128, under_nonuniform_scale),
        ops("stroke/under_rotation", Geometry, P1, 128, 128, under_rotation),
        ops("stroke/hairline_grid", Geometry, P1, 128, 128, hairline_grid),
    ];
    base.into_iter().flat_map(with_scale_variants).collect()
}

fn s(p: &mut Painter<'_>, st: &Stroke, t: Affine, color: Color, shape: &impl Shape) {
    p.stroke(st, t, color, None, shape);
}

fn polyline(pts: &[(f64, f64)]) -> BezPath {
    let mut path = BezPath::new();
    path.move_to(pts[0]);
    for &q in &pts[1..] {
        path.line_to(q);
    }
    path
}

/// Horizontal, 45° and vertical lines for each width.
fn widths(p: &mut Painter<'_>) {
    for (i, w) in [0.25, 0.5, 1., 1.5, 2., 8.].into_iter().enumerate() {
        let st = Stroke::new(w);
        let y = 10. + 18. * i as f64;
        s(p, &st, Affine::IDENTITY, INK, &Line::new((6., y), (40., y)));
        s(p, &st, Affine::IDENTITY, INK, &Line::new((46., y - 6.), (58., y + 6.)));
        let x = 72. + 9. * i as f64;
        s(p, &st, Affine::IDENTITY, INK, &Line::new((x, 8.), (x, 120.)));
    }
}

/// A 5-vertex zigzag per row: Miter, Round, Bevel.
fn joins(p: &mut Painter<'_>) {
    for (row, join) in [Join::Miter, Join::Round, Join::Bevel].into_iter().enumerate() {
        let y = 24. + 40. * row as f64;
        let zig =
            polyline(&[(16., y + 10.), (40., y - 10.), (64., y + 10.), (88., y - 10.), (112., y + 10.)]);
        s(p, &Stroke::new(12.).with_join(join), Affine::IDENTITY, INK, &zig);
    }
}

fn caps(p: &mut Painter<'_>) {
    for (row, cap) in [Cap::Butt, Cap::Round, Cap::Square].into_iter().enumerate() {
        let y = 32. + 32. * row as f64;
        s(p, &Stroke::new(12.).with_caps(cap), Affine::IDENTITY, INK, &Line::new((24., y), (104., y)));
    }
}

/// Each dash pattern (offset 2.5, round caps) on a rect and a circle.
fn dashes(p: &mut Painter<'_>) {
    let patterns: [&[f64]; 3] = [&[6., 3.], &[1., 1.], &[8., 4., 2., 4.]];
    for (row, pat) in patterns.into_iter().enumerate() {
        let st = Stroke::new(2.).with_caps(Cap::Round).with_dashes(2.5, pat.iter().copied());
        let y = 8. + 40. * row as f64;
        s(p, &st, Affine::IDENTITY, INK, &Rect::new(8., y, 56., y + 32.));
        s(p, &st, Affine::IDENTITY, INK, &Circle::new((92., y + 16.), 15.));
    }
}

/// 15° zigzags (each segment 7.5° off the row) with miter limits 1, 4, 10.
fn miter_limit(p: &mut Painter<'_>) {
    let rise = 96. * 7.5f64.to_radians().tan();
    for (row, limit) in [1., 4., 10.].into_iter().enumerate() {
        let y = 8. + 40. * row as f64;
        let zig = polyline(&[(16., y), (112., y + rise), (16., y + 2. * rise)]);
        let st = Stroke::new(4.).with_join(Join::Miter).with_miter_limit(limit);
        s(p, &st, Affine::IDENTITY, INK, &zig);
    }
}

fn curve(p: &mut Painter<'_>) {
    let mut path = BezPath::new();
    path.move_to((16., 100.));
    path.curve_to((40., 0.), (88., 128.), (112., 28.));
    s(p, &Stroke::new(3.), Affine::IDENTITY, INK, &path);
}

/// The decorated-box border.
fn closed_rrect_border(p: &mut Painter<'_>) {
    s(p, &Stroke::new(1.), Affine::IDENTITY, INK, &RoundedRect::new(10.5, 10.5, 117.5, 117.5, 12.));
}

/// Local-space stroking: the rrect maps to (16,16)–(112,112) on the canvas.
fn under_nonuniform_scale(p: &mut Painter<'_>) {
    let t = Affine::scale_non_uniform(2.0, 0.5);
    s(p, &Stroke::new(2.), t, INK, &RoundedRect::new(8., 32., 56., 224., 8.));
}

#[allow(clippy::approx_constant)] // 0.5236 is the §7.2 construction verbatim, not FRAC_PI_6
fn under_rotation(p: &mut Painter<'_>) {
    let t = Affine::rotate_about(0.5236, kurbo::Point::new(64., 64.));
    s(p, &Stroke::new(4.), t, INK, &Rect::new(32., 32., 96., 96.));
}

/// 1-px lines every 8 px on both axes, centered on pixel centers.
fn hairline_grid(p: &mut Painter<'_>) {
    let ink = Color::from_rgba8(0, 0, 0, 0x80);
    for k in 0..16 {
        let v = 8. * f64::from(k) + 4.5;
        s(p, &Stroke::new(1.), Affine::IDENTITY, ink, &Line::new((0., v), (128., v)));
        s(p, &Stroke::new(1.), Affine::IDENTITY, ink, &Line::new((v, 0.), (v, 128.)));
    }
}
