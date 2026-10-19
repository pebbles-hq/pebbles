//! `layer/` group opacity (§7, 09 §8.0) — Slice P4. Opacity is exact: a group composites once
//! (Flutter `Opacity` / Skia `saveLayerAlpha` semantics), a single draw folds its alpha.
//!
//! Colors are chosen so no 8-bit result lands exactly on a .5 tie: float→unorm rounding of a
//! tie is implementation-defined (Vulkan §3.9.3), so a tie would judge the driver, not the
//! renderer. At alpha 0.5 over white a channel `c` gives `c/2 + 127.5`, so the 0.5 cases use
//! odd channels; the nested case's inner content (stored at alpha 0.5 in the outer layer, then
//! composited at 0.5) uses channels ≡ 2 (mod 4).

use pebbles_core::IntoWidget as _;
use pebbles_foundation::Color as UiColor;
use pebbles_render::paint::{Painter, kurbo, peniko};
use pebbles_testing::Harness;
use pebbles_widgets::{container, opacity, positioned, stack};

use kurbo::{Affine, Rect, RoundedRect, Shape};
use peniko::{Color, Fill, Mix};

use crate::case::{Case, Category, Category::Layer, Slice::P4, harness, ops};
use crate::probe;

/// Every `layer/` opacity case.
pub fn cases() -> Vec<Case> {
    vec![
        harness("layer/opacity_0_5", Layer, P4, 128, 96, opacity_0_5),
        ops("layer/opacity_zero", Category::Analytic, P4, 128, 96, |p| {
            group(p, 0.0, &Rect::new(0., 0., 128., 96.), |p| fill(p, RED, Rect::new(16., 16., 112., 80.)))
        })
        .analytic()
        .probe("all_background", probe::all_background),
        ops("layer/opacity_nested", Layer, P4, 128, 96, |p| {
            group(p, 0.5, &Rect::new(0., 0., 128., 96.), |p| {
                fill(p, RED, Rect::new(8., 8., 40., 40.));
                fill(p, BLUE, Rect::new(24., 24., 56., 88.));
                group(p, 0.5, &Rect::new(0., 0., 128., 96.), |p| {
                    fill(p, RED_4K2, Rect::new(64., 8., 104., 48.));
                    fill(p, BLUE_4K2, Rect::new(80., 32., 120., 88.));
                });
            })
        }),
        ops("layer/opacity_clip", Layer, P4, 128, 96, |p| {
            group(p, 0.6, &RoundedRect::new(12., 12., 116., 84., 20.), |p| two_rects(p, 20.0))
        }),
        ops("layer/opacity_fold_single", Layer, P4, 128, 96, |p| {
            group(p, 0.5, &Rect::new(0., 0., 128., 96.), |p| {
                p.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    BLUE,
                    None,
                    &RoundedRect::new(16., 16., 112., 80., 16.),
                )
            })
        }),
    ]
}

const RED: Color = Color::from_rgba8(0xEF, 0x45, 0x45, 0xFF);
const BLUE: Color = Color::from_rgba8(0x3B, 0x83, 0xF7, 0xFF);
const RED_4K2: Color = Color::from_rgba8(0xF6, 0x46, 0x46, 0xFF);
const BLUE_4K2: Color = Color::from_rgba8(0x3A, 0x82, 0xF6, 0xFF);

fn fill(p: &mut Painter<'_>, c: Color, r: Rect) {
    p.fill(Fill::NonZero, Affine::IDENTITY, c, None, &r);
}

/// Two overlapping opaque rects shifted by `dx` (the overlap must not double-blend).
fn two_rects(p: &mut Painter<'_>, dx: f64) {
    fill(p, RED, Rect::new(8. + dx, 8., 56. + dx, 56.));
    fill(p, BLUE, Rect::new(32. + dx, 32., 80. + dx, 88.));
}

fn group(p: &mut Painter<'_>, alpha: f32, clip: &impl Shape, body: impl FnOnce(&mut Painter<'_>)) {
    p.push_layer(Fill::NonZero, Mix::Normal, alpha, Affine::IDENTITY, clip);
    body(p);
    p.pop_layer();
}

/// 05 §7: `opacity(0.5, stack of two overlapping boxes)`.
fn opacity_0_5(h: &mut Harness) {
    let a = container().width(64.).height(48.).color(UiColor::from_rgba8(0xEF, 0x45, 0x45, 0xFF));
    let b = container().width(64.).height(48.).color(UiColor::from_rgba8(0x3B, 0x83, 0xF7, 0xFF));
    h.mount_widget(opacity(
        0.5,
        stack(vec![
            positioned(a).left(16.).top(16.).into_widget(),
            positioned(b).left(48.).top(32.).into_widget(),
        ]),
    ));
}
