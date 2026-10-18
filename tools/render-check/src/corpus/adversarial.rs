//! adversarial/ — Slice Robustness, Analytic (§7.2, V26): invalid input must be skipped or
//! clamped without panics, hangs or garbage. Every case also draws one valid rect so the
//! probe has a known valid region; the corner patches stay background.

use pebbles_core::IntoWidget as _;
use pebbles_render::paint::{Painter, kurbo, peniko};
use pebbles_testing::Harness;

use kurbo::{Affine, BezPath, Cap, Line, Rect, Stroke};
use peniko::{Color, Fill, Mix};

use crate::case::{Case, Category::Analytic, Slice::Robustness, harness, ops};
use crate::probe;

const W: u32 = 256;
const H: u32 = 256;

pub fn cases() -> Vec<Case> {
    let a = |c: Case| c.analytic().probe("adversarial_bg_outside_valid", probe::adversarial_bg_outside_valid);
    vec![
        a(ops("adversarial/nan_inf_geometry", Analytic, Robustness, W, H, nan_inf_geometry)),
        a(ops("adversarial/zero_size", Analytic, Robustness, W, H, zero_size)),
        a(ops("adversarial/huge_stroke_10000", Analytic, Robustness, W, H, huge_stroke_10000)),
        a(ops("adversarial/dash_degenerate", Analytic, Robustness, W, H, dash_degenerate)),
        a(ops("adversarial/huge_coords", Analytic, Robustness, W, H, huge_coords)),
        // A 2000-px glyph covers the canvas: only the opacity half of the probe applies.
        harness("adversarial/glyph_2000px", Analytic, Robustness, W, H, glyph_2000px)
            .analytic()
            .probe("all_opaque", probe::all_opaque),
        a(ops("adversarial/clip_empty_intersection", Analytic, Robustness, W, H, clip_empty_intersection)),
    ]
}

const INK: Color = Color::from_rgb8(37, 99, 235);

/// The one valid shape every case draws (away from the 4×4 corner patches).
fn valid(p: &mut Painter<'_>) {
    p.fill(Fill::NonZero, Affine::IDENTITY, INK, None, &Rect::new(96.0, 96.0, 160.0, 160.0));
}

fn nan_inf_geometry(p: &mut Painter<'_>) {
    valid(p);
    p.fill(Fill::NonZero, Affine::IDENTITY, INK, None, &Rect::new(f64::NAN, 10.0, 50.0, 50.0));
    let mut path = BezPath::new();
    path.move_to((10.0, 10.0));
    path.line_to((f64::INFINITY, 20.0));
    path.line_to((30.0, 40.0));
    path.close_path();
    p.fill(Fill::NonZero, Affine::IDENTITY, INK, None, &path);
    let nan = Affine::new([f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0]);
    p.fill(Fill::NonZero, nan, INK, None, &Rect::new(0.0, 0.0, 256.0, 256.0));
}

fn zero_size(p: &mut Painter<'_>) {
    valid(p);
    p.fill(Fill::NonZero, Affine::IDENTITY, INK, None, &Rect::new(20.0, 20.0, 20.0, 80.0));
    p.fill(Fill::NonZero, Affine::IDENTITY, INK, None, &BezPath::new());
    let round = Stroke::new(6.0).with_caps(Cap::Round);
    p.stroke(&round, Affine::IDENTITY, INK, None, &Line::new((40.0, 40.0), (40.0, 40.0)));
    let empty = peniko::ImageData {
        data: peniko::Blob::from(Vec::<u8>::new()),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::Alpha,
        width: 0,
        height: 0,
    };
    p.draw_image(&peniko::ImageBrush::new(empty), Affine::translate((60.0, 60.0)));
}

fn huge_stroke_10000(p: &mut Painter<'_>) {
    // The stroke covers the canvas, so it is clipped to keep the corners as background.
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &Rect::new(32.0, 32.0, 224.0, 224.0));
    let seg = Line::new((120.0, 128.0), (130.0, 128.0));
    p.stroke(&Stroke::new(10_000.0), Affine::IDENTITY, INK, None, &seg);
    p.pop_layer();
}

fn dash_degenerate(p: &mut Painter<'_>) {
    valid(p);
    let a = Line::new((10.0, 200.0), (2010.0, 200.0));
    let b = Line::new((10.0, 220.0), (2010.0, 220.0));
    p.stroke(&Stroke::new(2.0).with_dashes(0.0, [0.0, 0.0]), Affine::IDENTITY, INK, None, &a);
    p.stroke(&Stroke::new(2.0).with_dashes(0.0, [1e-6, 1e-6]), Affine::IDENTITY, INK, None, &b);
}

fn huge_coords(p: &mut Painter<'_>) {
    valid(p);
    p.fill(Fill::NonZero, Affine::IDENTITY, INK, None, &Rect::new(1e9, 10.0, 1e9 + 50.0, 60.0));
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &Rect::new(-1e12, -1e12, 1e12, 1e12));
    p.fill(Fill::NonZero, Affine::IDENTITY, INK, None, &Rect::new(100.0, 170.0, 150.0, 190.0));
    p.pop_layer();
}

fn glyph_2000px(h: &mut Harness) {
    h.mount_widget(pebbles_widgets::text("W").size(2000.0).into_widget());
}

fn clip_empty_intersection(p: &mut Painter<'_>) {
    valid(p);
    for d in 0..8 {
        let off = if d % 2 == 0 { 0.0 } else { 120.0 };
        let r = Rect::new(off, off, off + 100.0, off + 100.0);
        p.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &r);
    }
    let red = Color::from_rgb8(220, 38, 38);
    p.fill(Fill::NonZero, Affine::IDENTITY, red, None, &Rect::new(0.0, 0.0, 256.0, 256.0));
    for _ in 0..8 {
        p.pop_layer();
    }
}
