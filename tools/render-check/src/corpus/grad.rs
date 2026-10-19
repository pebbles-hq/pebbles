//! `grad/` gradients (05 §7, 09 §8.0) — Slice P4, 128×128 unless stated. Stops default
//! `[#6366F1, #EC4899]`. Gradients are dithered (09 §5.8); the golden model dithers identically.

use pebbles_core::IntoWidget as _;
use pebbles_render::paint::{Glyph, Painter, kurbo, peniko};
use pebbles_render::{BorderRadius, BoxDecoration};
use pebbles_testing::Harness;
use pebbles_widgets::container;

use kurbo::{Affine, Rect, Stroke};
use peniko::color::ColorSpaceTag;
use peniko::{Brush, Color, Extend, Fill, Gradient, InterpolationAlphaSpace};

use crate::case::{Case, Category, Category::Gradient as Grad, Slice::P4, harness, ops};

const A: Color = Color::from_rgba8(0x63, 0x66, 0xF1, 0xFF);
const B: Color = Color::from_rgba8(0xEC, 0x48, 0x99, 0xFF);
const S: f64 = 128.0;

fn stops(g: Gradient) -> Gradient {
    g.with_stops([A, B].as_slice())
}

fn canvas(p: &mut Painter<'_>, g: &Gradient) {
    p.fill(Fill::NonZero, Affine::IDENTITY, g, None, &Rect::new(0., 0., S, S));
}

macro_rules! extends {
    ($v:ident, $name:literal, $make:expr) => {
        $v.extend([
            ops(concat!("grad/", $name, "_pad"), Grad, P4, 128, 128, |p| {
                canvas(p, &stops($make).with_extend(Extend::Pad))
            }),
            ops(concat!("grad/", $name, "_repeat"), Grad, P4, 128, 128, |p| {
                canvas(p, &stops($make).with_extend(Extend::Repeat))
            }),
            ops(concat!("grad/", $name, "_reflect"), Grad, P4, 128, 128, |p| {
                canvas(p, &stops($make).with_extend(Extend::Reflect))
            }),
        ]);
    };
}

/// Every `grad/` case.
pub fn cases() -> Vec<Case> {
    let mut v = Vec::new();
    extends!(v, "linear", Gradient::new_linear((32., 0.), (96., 0.)));
    extends!(v, "radial", Gradient::new_radial((64., 64.), 24.));
    extends!(v, "two_point", Gradient::new_two_point_radial((48., 64.), 8., (80., 64.), 40.));
    extends!(v, "sweep", Gradient::new_sweep((64., 64.), 0.0, std::f32::consts::PI));
    v.extend([
        ops("grad/linear_multistop", Grad, P4, 128, 128, |p| {
            let g = Gradient::new_linear((0., 0.), (S, 0.)).with_stops(
                [
                    (0.0, A),
                    (0.1, Color::WHITE),
                    (0.45, B),
                    (0.5, Color::BLACK),
                    (1.0, Color::from_rgba8(0x10, 0xB9, 0x81, 0xFF)),
                ]
                .as_slice(),
            );
            canvas(p, &g);
        }),
        ops("grad/linear_alpha_stops", Grad, P4, 128, 128, |p| {
            checker(p);
            let g = Gradient::new_linear((0., 0.), (S, 0.))
                .with_stops([Color::from_rgba8(255, 0, 0, 0), Color::from_rgba8(255, 0, 0, 255)].as_slice());
            canvas(p, &g);
        }),
        ops("grad/hard_stops", Grad, P4, 128, 128, |p| {
            let red = Color::from_rgba8(255, 0, 0, 255);
            let blue = Color::from_rgba8(0, 0, 255, 255);
            canvas(
                p,
                &Gradient::new_linear((0., 0.), (S, 0.))
                    .with_stops([(0.0, red), (0.5, red), (0.5, blue), (1.0, blue)].as_slice()),
            );
        }),
        ops("grad/brush_transform_rotate30", Grad, P4, 128, 128, |p| {
            let g = stops(Gradient::new_linear((0., 0.), (S, 0.)));
            p.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                &g,
                Some(Affine::rotate(std::f64::consts::FRAC_PI_6)),
                &Rect::new(0., 0., S, S),
            );
        }),
        ops("grad/stroke_gradient", Grad, P4, 128, 128, |p| {
            let g = stops(Gradient::new_linear((16., 0.), (112., 0.)));
            p.stroke(&Stroke::new(8.0), Affine::IDENTITY, &g, None, &kurbo::Circle::new((64., 64.), 44.));
        }),
        harness("grad/decoration_gradient_rrect", Grad, P4, 160, 120, decoration_gradient_rrect),
        ops("grad/interpolation_cs", Grad, P4, 128, 128, |p| {
            let mut g = Gradient::new_linear((0., 0.), (S, 0.)).with_stops(
                [Color::from_rgba8(255, 0, 0, 255), Color::from_rgba8(0, 0, 255, 255)].as_slice(),
            );
            g.interpolation_cs = ColorSpaceTag::Oklab;
            canvas(p, &g);
        }),
        ops("grad/gradient_text", Category::Text, P4, 200, 64, gradient_text),
        // 09 §8.0 NEW cases.
        ops("grad/radial_strip", Grad, P4, 128, 128, |p| {
            canvas(p, &stops(Gradient::new_two_point_radial((32., 64.), 20., (96., 64.), 20.)))
        }),
        ops("grad/radial_focal_on_circle", Grad, P4, 128, 128, |p| {
            canvas(p, &stops(Gradient::new_two_point_radial((104., 64.), 0., (64., 64.), 40.)))
        }),
        ops("grad/sweep_negative_start", Grad, P4, 128, 128, |p| {
            canvas(
                p,
                &stops(Gradient::new_sweep(
                    (64., 64.),
                    -std::f32::consts::FRAC_PI_2,
                    3.0 * std::f32::consts::FRAC_PI_2,
                )),
            )
        }),
        ops("grad/unpremul_alpha", Grad, P4, 128, 128, |p| {
            checker(p);
            let mut g = Gradient::new_linear((0., 0.), (S, 0.))
                .with_stops([Color::from_rgba8(255, 0, 0, 0), Color::from_rgba8(0, 0, 255, 255)].as_slice());
            g.interpolation_alpha_space = InterpolationAlphaSpace::Unpremultiplied;
            canvas(p, &g);
        }),
        ops("grad/degenerate", Grad, P4, 128, 128, degenerate),
    ]);
    v
}

/// A 16-px gray checker under translucent gradients.
fn checker(p: &mut Painter<'_>) {
    for y in 0..8 {
        for x in 0..8 {
            if (x + y) % 2 == 0 {
                let r = Rect::new(
                    f64::from(x) * 16.,
                    f64::from(y) * 16.,
                    f64::from(x + 1) * 16.,
                    f64::from(y + 1) * 16.,
                );
                p.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgba8(200, 200, 200, 255), None, &r);
            }
        }
    }
}

/// 05 §7: `BoxDecoration::new().gradient(..).radius(BorderRadius::all(16.))`.
fn decoration_gradient_rrect(h: &mut Harness) {
    use pebbles_foundation::Alignment;
    let g = pebbles_render::Gradient::linear(Alignment::TOP_LEFT, Alignment::BOTTOM_RIGHT, [A, B]);
    let deco = BoxDecoration::new().gradient(g).radius(BorderRadius::all(16.));
    h.mount_widget(container().width(120.).height(80.).decoration(deco).into_widget());
}

/// 05 §7: a gradient brush on a glyph run (drawn through outlines, L36).
fn gradient_text(p: &mut Painter<'_>) {
    let font =
        peniko::FontData::new(peniko::Blob::from(pebbles_render::fonts::builtin_fonts()[0].1.to_vec()), 0);
    let Ok(f) = skrifa::FontRef::from_index(font.data.data(), 0) else { return };
    let cmap = skrifa::MetadataProvider::charmap(&f);
    let g = stops(Gradient::new_linear((0., 0.), (200., 0.)));
    let glyphs: Vec<Glyph> = "Gradient"
        .chars()
        .enumerate()
        .filter_map(|(i, c)| {
            cmap.map(c).map(|id| Glyph { id: id.to_u32(), x: 8.0 + 23.0 * i as f32, y: 46.0 })
        })
        .collect();
    p.draw_glyphs(
        &font,
        36.0,
        &[],
        &Brush::Gradient(g),
        Affine::IDENTITY,
        None,
        Fill::NonZero,
        glyphs.into_iter(),
    );
}

/// 09 §8.0: four panels — no stops, one stop, start == end, sweep end ≤ start (each draws
/// its §5.1 fallback solid).
fn degenerate(p: &mut Painter<'_>) {
    let panel = |i: u32| {
        let (x, y) = (f64::from(i % 2) * 64., f64::from(i / 2) * 64.);
        Rect::new(x + 4., y + 4., x + 60., y + 60.)
    };
    let gs = [
        Gradient::new_linear((0., 0.), (S, 0.)),
        Gradient::new_linear((0., 0.), (S, 0.)).with_stops([B].as_slice()),
        stops(Gradient::new_linear((10., 10.), (10., 10.))),
        stops(Gradient::new_sweep((96., 96.), 1.0, 1.0)),
    ];
    for (i, g) in gs.iter().enumerate() {
        p.fill(Fill::NonZero, Affine::IDENTITY, g, None, &panel(i as u32));
    }
}
