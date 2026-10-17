//! `prim/` — filled primitives (§7.2) — Slice P1 — 128×128.

use std::f64::consts::{FRAC_PI_2, TAU};

use pebbles_icons::tabler;
use pebbles_render::paint::{Painter, kurbo, peniko};
use pebbles_testing::Harness;

use kurbo::{Affine, BezPath, Circle, Ellipse, Rect, RoundedRect, RoundedRectRadii, Shape};
use peniko::{Color, Fill};

use crate::case::{Case, Category::Geometry, Slice::P1, harness, ops, with_scale_variants};

/// `#3366FF`, the prim/ fill color unless a case names another.
pub const BLUE: Color = Color::from_rgba8(0x33, 0x66, 0xFF, 0xFF);

/// Every `prim/` case.
pub fn cases() -> Vec<Case> {
    let mut v = vec![
        ops("prim/rect_aligned", crate::case::Category::Exact, P1, 128, 128, rect_aligned),
        ops("prim/rect_fractional", Geometry, P1, 128, 128, rect_fractional),
        ops("prim/rect_translucent_overlap", Geometry, P1, 128, 128, rect_translucent_overlap),
        ops("prim/rrect_radii", Geometry, P1, 128, 128, rrect_radii),
        ops("prim/rrect_per_corner", Geometry, P1, 128, 128, rrect_per_corner),
    ];
    v.extend(with_scale_variants(ops("prim/circles", Geometry, P1, 128, 128, circles)));
    v.extend([
        ops("prim/ellipse_rotated", Geometry, P1, 128, 128, ellipse_rotated),
        ops("prim/star_nonzero", Geometry, P1, 128, 128, star_nonzero),
        ops("prim/star_evenodd", Geometry, P1, 128, 128, star_evenodd),
        ops("prim/donut_nonzero", Geometry, P1, 128, 128, donut_nonzero),
        ops("prim/donut_evenodd", Geometry, P1, 128, 128, donut_evenodd),
        ops("prim/curves", Geometry, P1, 128, 128, curves),
        ops("prim/slivers", Geometry, P1, 128, 128, slivers),
        ops("prim/many_subpaths", Geometry, P1, 128, 128, many_subpaths),
        harness("prim/icons", Geometry, P1, 256, 160, icons),
    ]);
    v
}

fn fill(p: &mut Painter<'_>, style: Fill, color: Color, shape: &impl Shape) {
    p.fill(style, Affine::IDENTITY, color, None, shape);
}

fn rect_aligned(p: &mut Painter<'_>) {
    fill(p, Fill::NonZero, BLUE, &Rect::new(16., 16., 112., 112.));
}

fn rect_fractional(p: &mut Painter<'_>) {
    fill(p, Fill::NonZero, BLUE, &Rect::new(16.25, 16.5, 111.75, 111.25));
}

fn rect_translucent_overlap(p: &mut Painter<'_>) {
    fill(p, Fill::NonZero, Color::from_rgba8(0xFF, 0, 0, 0x80), &Rect::new(16., 16., 80., 80.));
    fill(p, Fill::NonZero, Color::from_rgba8(0, 0, 0xFF, 0x80), &Rect::new(48., 48., 112., 112.));
}

fn rrect_radii(p: &mut Painter<'_>) {
    let green = Color::from_rgba8(0x10, 0xB9, 0x81, 0xFF);
    for (i, r) in [0., 4., 12., 26.].into_iter().enumerate() {
        let (x0, y0) = (8. + 60. * (i % 2) as f64, 8. + 60. * (i / 2) as f64);
        fill(p, Fill::NonZero, green, &RoundedRect::new(x0, y0, x0 + 52., y0 + 52., r));
    }
}

fn rrect_per_corner(p: &mut Painter<'_>) {
    let radii = RoundedRectRadii::new(0., 8., 24., 40.);
    fill(p, Fill::NonZero, BLUE, &RoundedRect::from_rect(Rect::new(16., 16., 112., 112.), radii));
}

fn circles(p: &mut Painter<'_>) {
    let pink = Color::from_rgba8(0xEC, 0x48, 0x99, 0xFF);
    for (c, r) in [((20., 20.), 1.5), ((40., 20.), 4.), ((40., 80.), 16.), ((88., 72.), 36.)] {
        fill(p, Fill::NonZero, pink, &Circle::new(c, r));
    }
}

fn ellipse_rotated(p: &mut Painter<'_>) {
    fill(p, Fill::NonZero, BLUE, &Ellipse::new((64., 64.), (50., 24.), 0.3));
}

/// Pentagram: 5 points on `r` around `c` (first at the top), connected every 2nd point.
pub fn star(c: (f64, f64), r: f64) -> BezPath {
    let pt = |k: usize| {
        let a = -FRAC_PI_2 + TAU * k as f64 / 5.;
        (c.0 + r * a.cos(), c.1 + r * a.sin())
    };
    let mut path = BezPath::new();
    path.move_to(pt(0));
    for k in [2, 4, 1, 3] {
        path.line_to(pt(k));
    }
    path.close_path();
    path
}

fn star_nonzero(p: &mut Painter<'_>) {
    fill(p, Fill::NonZero, BLUE, &star((64., 64.), 52.));
}

fn star_evenodd(p: &mut Painter<'_>) {
    fill(p, Fill::EvenOdd, BLUE, &star((64., 64.), 52.));
}

fn donut() -> BezPath {
    let mut path = Circle::new((64., 64.), 48.).to_path(1e-3);
    path.extend(Circle::new((64., 64.), 24.).path_elements(1e-3));
    path
}

fn donut_nonzero(p: &mut Painter<'_>) {
    fill(p, Fill::NonZero, BLUE, &donut());
}

fn donut_evenodd(p: &mut Painter<'_>) {
    fill(p, Fill::EvenOdd, BLUE, &donut());
}

fn curves(p: &mut Painter<'_>) {
    let mut path = BezPath::new();
    path.move_to((16., 112.));
    path.quad_to((64., 0.), (112., 112.));
    path.curve_to((96., 64.), (32., 64.), (16., 112.));
    path.close_path();
    fill(p, Fill::NonZero, BLUE, &path);
}

fn slivers(p: &mut Painter<'_>) {
    for (i, w) in [0.25, 0.5, 0.75, 1.0].into_iter().enumerate() {
        let x = 10. + 25. * i as f64;
        fill(p, Fill::NonZero, BLUE, &Rect::new(x, 14., x + w, 114.));
    }
}

fn many_subpaths(p: &mut Painter<'_>) {
    let mut path = BezPath::new();
    for i in 0..100 {
        let (x, y) = (5. + 12. * (i % 10) as f64, 5. + 12. * (i / 10) as f64);
        path.extend(Rect::new(x, y, x + 6., y + 6.).path_elements(0.1));
    }
    fill(p, Fill::NonZero, BLUE, &path);
}

fn icons(h: &mut Harness) {
    use pebbles_core::IntoWidget as _;
    use pebbles_widgets::{icon, wrap};
    let set = [
        tabler::HOME,
        tabler::SEARCH,
        tabler::SETTINGS,
        tabler::STAR,
        tabler::HEART,
        tabler::BELL,
        tabler::USER,
        tabler::MAIL,
        tabler::CALENDAR,
        tabler::TRASH,
        tabler::EDIT,
        tabler::CHECK,
        tabler::X,
        tabler::PLUS,
        tabler::MINUS,
        tabler::ARROW_LEFT,
        tabler::ARROW_RIGHT,
        tabler::CHEVRON_DOWN,
        tabler::MENU_2,
        tabler::FILTER,
        tabler::DOWNLOAD,
        tabler::UPLOAD,
        tabler::LOCK,
        tabler::EYE,
        tabler::filled::STAR,
    ];
    let sizes = [16., 24., 48.];
    let children: Vec<_> =
        set.into_iter().enumerate().map(|(i, d)| icon(d).size(sizes[i % 3]).into_widget()).collect();
    h.mount_widget(wrap(children));
}
