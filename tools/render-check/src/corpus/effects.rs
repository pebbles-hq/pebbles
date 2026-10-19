//! `effect/` layer effects, backdrop filters and perspective layers (05 §7 V53, 09 §10) —
//! Slice P4, 128×96, judged by the golden model (Effect / Projected limits, 05 §8.1).

use pebbles_render::paint::display_list::{BlurEdge, LayerEffect};
use pebbles_render::paint::{ColorMatrix, Glyph, Painter, Projective, kurbo, peniko};

use kurbo::{Affine, Rect, RoundedRect};
use peniko::{Brush, Color, Fill, Mix};

use crate::case::{Case, Category::Effect, Category::Projected, Slice::P4, ops};

const W: u32 = 128;
const H: u32 = 96;

pub fn cases() -> Vec<Case> {
    vec![
        ops("effect/blur_square", Effect, P4, W, H, |p| {
            layer(p, blur(4.0), |p| fill(p, 0x3B82F6, Rect::new(40., 24., 88., 72.)));
        }),
        ops("effect/blur_text", Effect, P4, W, H, |p| layer(p, blur(1.5), text)),
        ops("effect/backdrop_frosted", Effect, P4, W, H, |p| {
            stripes(p);
            p.push_backdrop_filter(
                blur(3.0),
                &RoundedRect::new(24., 16., 104., 80., 12.),
                Affine::IDENTITY,
                Mix::Normal,
                1.0,
            );
            fill(p, 0xFFFFFF, Rect::new(40., 40., 88., 56.));
            p.pop_layer();
        }),
        ops("effect/backdrop_nested", Effect, P4, W, H, |p| {
            stripes(p);
            p.push_backdrop_filter(
                blur(1.5),
                &Rect::new(16., 12., 112., 84.),
                Affine::IDENTITY,
                Mix::Normal,
                1.0,
            );
            p.push_backdrop_filter(
                blur(1.5),
                &Rect::new(40., 28., 88., 68.),
                Affine::IDENTITY,
                Mix::Normal,
                1.0,
            );
            p.pop_layer();
            p.pop_layer();
        }),
        ops("effect/color_matrix_grayscale", Effect, P4, W, H, |p| {
            layer(p, ColorMatrix::grayscale().into(), bars)
        }),
        ops("effect/color_matrix_alpha_offset", Effect, P4, W, H, |p| {
            let mut m = ColorMatrix::identity().0;
            m[18] = 0.5;
            m[19] = 0.25;
            layer(p, LayerEffect::ColorMatrix(m), |p| fill(p, 0xEF4444, Rect::new(16., 16., 64., 80.)));
        }),
        ops("effect/dilate", Effect, P4, W, H, |p| {
            layer(p, LayerEffect::Dilate { radius_x: 3., radius_y: 2. }, text);
        }),
        ops("effect/erode", Effect, P4, W, H, |p| {
            layer(p, LayerEffect::Erode { radius_x: 2., radius_y: 2. }, bars);
        }),
        ops("effect/chain_blur_matrix", Effect, P4, W, H, |p| {
            layer(p, LayerEffect::chain([blur(2.0), ColorMatrix::sepia().into()]), bars);
        }),
        ops("effect/perspective_card", Projected, P4, W, H, |p| {
            p.push_projected_layer(tilt(), Rect::new(0., 0., 80., 56.), None::<&Rect>);
            fill(p, 0x10B981, Rect::new(0., 0., 80., 56.));
            fill(p, 0xF59E0B, Rect::new(8., 8., 40., 28.));
            p.pop_layer();
        }),
        ops("effect/perspective_text", Projected, P4, W, H, |p| {
            p.push_projected_layer(tilt(), Rect::new(0., 0., 80., 56.), None::<&Rect>);
            fill(p, 0xF9FAFB, Rect::new(0., 0., 80., 56.));
            text(p);
            p.pop_layer();
        }),
    ]
}

fn blur(sigma: f32) -> LayerEffect {
    LayerEffect::Blur { sigma_x: sigma, sigma_y: sigma, edge: BlurEdge::Decal }
}

fn rgb(c: u32) -> Color {
    Color::from_rgba8((c >> 16) as u8, (c >> 8) as u8, c as u8, 255)
}

fn fill(p: &mut Painter<'_>, c: u32, r: Rect) {
    p.fill(Fill::NonZero, Affine::IDENTITY, rgb(c), None, &r);
}

fn layer(p: &mut Painter<'_>, e: LayerEffect, body: impl FnOnce(&mut Painter<'_>)) {
    p.push_effect_layer(e, None::<&Rect>, Affine::IDENTITY);
    body(p);
    p.pop_layer();
}

/// 05 §7 backdrop bars.
fn bars(p: &mut Painter<'_>) {
    for (i, c) in [0xEF4444, 0x10B981, 0x3B82F6, 0xF59E0B].into_iter().enumerate() {
        let x = 32.0 * i as f64;
        fill(p, c, Rect::new(x, 0., x + 32., 96.));
    }
}

/// 4-px black / white vertical stripes.
fn stripes(p: &mut Painter<'_>) {
    for i in 0..32 {
        if i % 2 == 0 {
            let x = 4.0 * f64::from(i);
            fill(p, 0x111827, Rect::new(x, 0., x + 4., 96.));
        }
    }
}

fn text(p: &mut Painter<'_>) {
    let font =
        peniko::FontData::new(peniko::Blob::from(pebbles_render::fonts::builtin_fonts()[0].1.to_vec()), 0);
    let Ok(f) = skrifa::FontRef::from_index(font.data.data(), 0) else { return };
    let cmap = skrifa::MetadataProvider::charmap(&f);
    let glyphs: Vec<Glyph> = "Pebbles"
        .chars()
        .enumerate()
        .filter_map(|(i, c)| {
            cmap.map(c).map(|id| Glyph { id: id.to_u32(), x: 6.0 + 16.0 * i as f32, y: 40.0 })
        })
        .collect();
    p.draw_glyphs(
        &font,
        24.0,
        &[],
        &Brush::Solid(rgb(0x111827)),
        Affine::IDENTITY,
        None,
        Fill::NonZero,
        glyphs.into_iter(),
    );
}

/// A card tilted about y around its center, placed at the canvas center.
fn tilt() -> Projective {
    let (s, c) = 0.6f64.sin_cos();
    let persp = Projective([c, 0.0, 0.0, 0.0, 1.0, 0.0, -0.004 * s, 0.0, 1.0]);
    Projective::from_affine(Affine::translate((-40., -28.)))
        .then(&persp)
        .then(&Projective::from_affine(Affine::translate((64., 48.))))
}
