//! `clip/` scissor subset (§7.2) — Slice P1 — 128×128.

use pebbles_core::IntoWidget as _;
use pebbles_foundation::Offset;
use pebbles_render::paint::{Painter, kurbo, peniko};
use pebbles_testing::Harness;

use kurbo::{Affine, Rect, RoundedRect, Shape};
use peniko::{Color, Fill, Mix};

use crate::case::{Case, Category, Category::Geometry, Slice::P1, harness, ops, with_scale_variants};
use crate::probe;

// §7.2 marks rect_fractional, rect_negative_origin, rect_offscreen, rrect_zero_radius and
// empty_intersection `xfail until C11`. C11 is done (5f18d75), so per §2.6 the markers are gone.

/// Every `clip/` scissor case.
pub fn cases() -> Vec<Case> {
    let mut v = vec![ops("clip/rect_scissor", Geometry, P1, 128, 128, rect_scissor)];
    // 09 §2: a fractional rect clip is a scissor rounded out until the SDF clip (L03/L04); the
    // golden model clips exactly, so these fail by design until then.
    v.extend(
        with_scale_variants(ops("clip/rect_fractional", Geometry, P1, 128, 128, rect_fractional))
            .into_iter()
            .map(|c| {
                c.xfail("L04", "fractional rect clips are scissors rounded out until the SDF clip (09 §2)")
            }),
    );
    v.extend([
        ops("clip/rect_negative_origin", Geometry, P1, 128, 128, rect_negative_origin),
        ops("clip/rect_offscreen", Category::Analytic, P1, 128, 128, rect_offscreen)
            .analytic()
            .probe("all_background", probe::all_background),
        ops("clip/rrect_zero_radius", Geometry, P1, 128, 128, rrect_zero_radius),
        ops("clip/empty_intersection", Category::Analytic, P1, 128, 128, empty_intersection)
            .analytic()
            .probe("all_background", probe::all_background),
        harness("clip/scroll_viewport", Geometry, P1, 240, 200, scroll_viewport)
            .xfail("V46", "the golden model renders glyph runs from V46"),
    ]);
    v
}

/// 8 vertical 16-px stripes of distinct colors across the whole canvas.
fn stripes(p: &mut Painter<'_>) {
    const COLORS: [u32; 8] = [0xEF4444, 0xF59E0B, 0x10B981, 0x3B82F6, 0x8B5CF6, 0xEC4899, 0x111827, 0x14B8A6];
    for (i, c) in COLORS.into_iter().enumerate() {
        let color = Color::from_rgba8((c >> 16) as u8, (c >> 8) as u8, c as u8, 0xFF);
        let x = 16. * i as f64;
        p.fill(Fill::NonZero, Affine::IDENTITY, color, None, &Rect::new(x, 0., x + 16., 128.));
    }
}

fn clipped(p: &mut Painter<'_>, clip: &impl Shape) {
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, clip);
    stripes(p);
    p.pop_layer();
}

fn rect_scissor(p: &mut Painter<'_>) {
    clipped(p, &Rect::new(20., 20., 100., 100.));
}

fn rect_fractional(p: &mut Painter<'_>) {
    clipped(p, &Rect::new(10.5, 10.5, 117.25, 117.75));
}

fn rect_negative_origin(p: &mut Painter<'_>) {
    clipped(p, &Rect::new(-20., -20., 64., 64.));
}

fn rect_offscreen(p: &mut Painter<'_>) {
    clipped(p, &Rect::new(200., 200., 300., 300.));
}

/// Must take the scissor path (DR-02 primitives).
fn rrect_zero_radius(p: &mut Painter<'_>) {
    clipped(p, &RoundedRect::new(20., 20., 100., 100., 0.));
}

fn empty_intersection(p: &mut Painter<'_>) {
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &Rect::new(0., 0., 40., 40.));
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &Rect::new(60., 60., 120., 120.));
    stripes(p);
    p.pop_layer();
    p.pop_layer();
}

/// A 40-row list in a 200-px-tall box, scrolled 37 px.
fn scroll_viewport(h: &mut Harness) {
    use pebbles_widgets::{container, list_tile, list_view};
    let rows: Vec<_> = (0..40).map(|i| list_tile(format!("Row {i}")).into_widget()).collect();
    h.mount_widget(container().height(200.).child(list_view(rows)));
    h.scroll(Offset::new(100., 100.), 37.);
}
