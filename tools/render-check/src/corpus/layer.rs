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
use pebbles_widgets::{color_filtered, container, opacity, positioned, stack};

use kurbo::{Affine, Rect, RoundedRect, Shape};
use peniko::{BlendMode, Color, Compose, Fill, Mix};

use crate::case::{Case, Category, Category::Layer, Slice::P4, harness, ops};
use crate::probe;

/// Every `layer/` case: opacity (L10), then blend and compose (L20).
pub fn cases() -> Vec<Case> {
    let mut v = opacity_cases();
    v.extend(MIXES.iter().map(|&(name, mix)| {
        ops(&format!("layer/blend_{name}"), Layer, P4, 128, 96, BLEND_FNS[mix as usize])
    }));
    v.extend(COMPOSES.iter().map(|&(name, c)| {
        ops(&format!("layer/compose_{name}"), Layer, P4, 128, 96, COMPOSE_FNS[c as usize])
    }));
    v.extend([
        harness("layer/color_filtered", Layer, P4, 160, 120, color_filtered_case),
        ops("layer/blend_inside_clip", Layer, P4, 128, 96, blend_inside_clip),
        ops("layer/blend_in_sdf_clip", Layer, P4, 128, 96, blend_in_sdf_clip),
        // 05 §7 `shader_mask_destin` (09 §8.0 `compose_destin_mask`): ShaderMask = a promoted
        // layer holding the child and a `(Normal, DestIn)` gradient layer.
        harness("layer/decoration_blend", Layer, P4, 160, 120, decoration_blend),
        // 05 §7: `Painter::append` of one sub-scene (layer + fills) at two transforms (L50).
        ops("layer/fragment_append", Layer, P4, 128, 96, |p| {
            let f = super::frag::tile();
            p.append(&f, Some(Affine::translate((8., 8.))));
            p.append(&f, Some(Affine::translate((64., 40.))));
        }),
        harness("layer/shader_mask_destin", Category::Analytic, P4, 200, 100, shader_mask_destin)
            .analytic()
            .probe("mask_fades_left_to_right", probe::mask_fades_left_to_right),
    ]);
    v
}

fn opacity_cases() -> Vec<Case> {
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

// ---- L20: blend and compose (05 §7, 09 §8.0) ----

const MIXES: [(&str, Mix); 16] = [
    ("normal", Mix::Normal),
    ("multiply", Mix::Multiply),
    ("screen", Mix::Screen),
    ("overlay", Mix::Overlay),
    ("darken", Mix::Darken),
    ("lighten", Mix::Lighten),
    ("color_dodge", Mix::ColorDodge),
    ("color_burn", Mix::ColorBurn),
    ("hard_light", Mix::HardLight),
    ("soft_light", Mix::SoftLight),
    ("difference", Mix::Difference),
    ("exclusion", Mix::Exclusion),
    ("hue", Mix::Hue),
    ("saturation", Mix::Saturation),
    ("color", Mix::Color),
    ("luminosity", Mix::Luminosity),
];

const COMPOSES: [(&str, Compose); 14] = [
    ("clear", Compose::Clear),
    ("copy", Compose::Copy),
    ("dest", Compose::Dest),
    ("src_over", Compose::SrcOver),
    ("dest_over", Compose::DestOver),
    ("src_in", Compose::SrcIn),
    ("dest_in", Compose::DestIn),
    ("src_out", Compose::SrcOut),
    ("dest_out", Compose::DestOut),
    ("src_atop", Compose::SrcAtop),
    ("dest_atop", Compose::DestAtop),
    ("xor", Compose::Xor),
    ("plus", Compose::Plus),
    ("plus_lighter", Compose::PlusLighter),
];

/// 05 §7 backdrop (128×96): 4 vertical bars and a gray ramp row.
fn backdrop(p: &mut Painter<'_>) {
    const BARS: [u32; 4] = [0xEF4444, 0x10B981, 0x3B82F6, 0xF59E0B];
    for (i, c) in BARS.into_iter().enumerate() {
        let x = 32.0 * i as f64;
        fill(p, rgb(c), Rect::new(x, 0., x + 32., 80.));
    }
    for i in 0..8 {
        let v = (i * 255 / 7) as u8;
        let x = 16.0 * f64::from(i);
        fill(p, Color::from_rgba8(v, v, v, 255), Rect::new(x, 80., x + 16., 96.));
    }
}

/// 05 §7 source: 3 horizontal bars `#8B5CF6 #EC4899 #FFFFFF80` drawn inside the layer.
fn source(p: &mut Painter<'_>) {
    fill(p, rgb(0x8B5CF6), Rect::new(8., 8., 120., 32.));
    fill(p, rgb(0xEC4899), Rect::new(8., 36., 120., 60.));
    fill(p, Color::from_rgba8(255, 255, 255, 0x80), Rect::new(8., 64., 120., 88.));
}

fn rgb(c: u32) -> Color {
    Color::from_rgba8((c >> 16) as u8, (c >> 8) as u8, c as u8, 255)
}

fn blend_case(p: &mut Painter<'_>, mix: Mix) {
    backdrop(p);
    p.push_layer(
        Fill::NonZero,
        BlendMode::new(mix, Compose::SrcOver),
        1.0,
        Affine::IDENTITY,
        &Rect::new(0., 0., 128., 96.),
    );
    source(p);
    p.pop_layer();
}

/// 09 §8.0: left half `#2060C0`, right half white; inside a promoted layer, a
/// `(Normal, compose)` layer with a circle `#E04020` α 0.75 over the middle.
fn compose_case(p: &mut Painter<'_>, compose: Compose) {
    let all = Rect::new(0., 0., 128., 96.);
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &all);
    fill(p, rgb(0x2060C0), Rect::new(0., 0., 64., 96.));
    p.push_layer(Fill::NonZero, BlendMode::new(Mix::Normal, compose), 1.0, Affine::IDENTITY, &all);
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0xE0, 0x40, 0x20, 0xBF),
        None,
        &kurbo::Circle::new((64., 48.), 36.),
    );
    p.pop_layer();
    p.pop_layer();
}

macro_rules! fns {
    ($f:ident, $t:ty, [$($v:expr),*]) => { [$(|p: &mut Painter<'_>| $f(p, $v)),*] };
}

const BLEND_FNS: [fn(&mut Painter<'_>); 16] = fns!(
    blend_case,
    Mix,
    [
        Mix::Normal,
        Mix::Multiply,
        Mix::Screen,
        Mix::Overlay,
        Mix::Darken,
        Mix::Lighten,
        Mix::ColorDodge,
        Mix::ColorBurn,
        Mix::HardLight,
        Mix::SoftLight,
        Mix::Difference,
        Mix::Exclusion,
        Mix::Hue,
        Mix::Saturation,
        Mix::Color,
        Mix::Luminosity
    ]
);

const COMPOSE_FNS: [fn(&mut Painter<'_>); 14] = fns!(
    compose_case,
    Compose,
    [
        Compose::Clear,
        Compose::Copy,
        Compose::Dest,
        Compose::SrcOver,
        Compose::DestOver,
        Compose::SrcIn,
        Compose::DestIn,
        Compose::SrcOut,
        Compose::DestOut,
        Compose::SrcAtop,
        Compose::DestAtop,
        Compose::Xor,
        Compose::Plus,
        Compose::PlusLighter
    ]
);

/// 05 §7: `color_filtered(#3B82F6, child)` (Multiply) over a two-color child.
fn color_filtered_case(h: &mut Harness) {
    let child = stack(vec![
        positioned(container().width(80.).height(80.).color(UiColor::from_rgba8(0xF5, 0x9E, 0x0B, 0xFF)))
            .left(10.)
            .top(10.)
            .into_widget(),
        positioned(container().width(60.).height(60.).color(UiColor::WHITE)).left(70.).top(40.).into_widget(),
    ]);
    h.mount_widget(color_filtered(UiColor::from_rgba8(0x3B, 0x82, 0xF6, 0xFF), child));
}

/// 09 §3.1: blue root; a pure rect clip; a gray fill over half the clip; a Multiply layer
/// filling the whole clip with red (promotion keeps the blue out of the multiply).
fn blend_inside_clip(p: &mut Painter<'_>) {
    fill(p, rgb(0x3B82F6), Rect::new(0., 0., 128., 96.));
    let clip = Rect::new(16., 16., 112., 80.);
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &clip);
    fill(p, Color::from_rgba8(0x80, 0x80, 0x80, 255), Rect::new(16., 16., 64., 80.));
    p.push_layer(Fill::NonZero, Mix::Multiply, 1.0, Affine::IDENTITY, &clip);
    fill(p, rgb(0xEF4444), clip);
    p.pop_layer();
    p.pop_layer();
}

/// 09 §8.0: an rrect clip (SDF) containing a `(Normal, DestIn)` layer → the backdrop path.
fn blend_in_sdf_clip(p: &mut Painter<'_>) {
    backdrop(p);
    let rr = RoundedRect::new(16., 12., 112., 84., 20.);
    p.push_layer(Fill::NonZero, BlendMode::new(Mix::Normal, Compose::DestIn), 1.0, Affine::IDENTITY, &rr);
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0, 0, 0, 0x99),
        None,
        &Rect::new(0., 0., 128., 96.),
    );
    p.pop_layer();
}

/// 05 §7: `shader_mask(white → transparent white over x 0..200, child)`.
fn shader_mask_destin(h: &mut Harness) {
    use pebbles_widgets::shader_mask;
    let [r, g, b, a] = probe::MASK_CHILD;
    let fade = pebbles_render::Gradient::horizontal([UiColor::WHITE, UiColor::from_rgba8(255, 255, 255, 0)]);
    let child = container().width(200.).height(100.).color(UiColor::from_rgba8(r, g, b, a));
    h.mount_widget(shader_mask(fade, child));
}

/// 05 §7: `BoxDecoration::new().blend(Multiply)` (a colored box) over an image.
fn decoration_blend(h: &mut Harness) {
    use pebbles_render::{BlendMode, BoxDecoration, image_from_rgba8};
    let px: Vec<u8> = (0..64u32 * 64)
        .flat_map(|i| {
            let (x, y) = (i % 64, i / 64);
            [(x * 4) as u8, (y * 4) as u8, 200, 255]
        })
        .collect();
    let image = BoxDecoration::new().image(image_from_rgba8(64, 64, px));
    let tint =
        BoxDecoration::new().color(UiColor::from_rgba8(0xF5, 0x9E, 0x0B, 0xFF)).blend(BlendMode::Multiply);
    h.mount_widget(stack(vec![
        positioned(container().width(140.).height(100.).decoration(image)).left(10.).top(10.).into_widget(),
        positioned(container().width(80.).height(60.).decoration(tint)).left(40.).top(30.).into_widget(),
    ]));
}
