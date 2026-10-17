//! Seeded property loop (06 §10.4): random op lists preserve painter's order.

use kurbo::{Affine, Rect};
use peniko::{Color, Fill};

use super::{PrepareInput, PrepareScratch, prepare};
use crate::test_support::{XorShift64, bundled_font, for_seeds};
use pebbles_render::paint::{Glyph, Painter, Scene, Stroke};

fn random_scene(rng: &mut XorShift64) -> Scene {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    let font = bundled_font();
    let rect = |rng: &mut XorShift64| {
        Rect::new(
            rng.range_f64(0.0, 256.0),
            rng.range_f64(0.0, 256.0),
            rng.range_f64(0.0, 256.0),
            rng.range_f64(0.0, 256.0),
        )
    };
    let color = |rng: &mut XorShift64| {
        Color::from_rgba8(
            u8::try_from(rng.range_u32(0, 255)).unwrap_or(0),
            u8::try_from(rng.range_u32(0, 255)).unwrap_or(0),
            u8::try_from(rng.range_u32(0, 255)).unwrap_or(0),
            255,
        )
    };
    for _ in 0..64 {
        match rng.range_u32(0, 7) {
            0 => p.fill(Fill::NonZero, Affine::IDENTITY, color(rng), None, &rect(rng)),
            1 => p.stroke(
                &Stroke::new(rng.range_f64(0.5, 8.0)),
                Affine::IDENTITY,
                color(rng),
                None,
                &rect(rng),
            ),
            2 => p.push_layer(Fill::NonZero, peniko::BlendMode::default(), 1.0, Affine::IDENTITY, &rect(rng)),
            3 => p.pop_layer(),
            4 => p.draw_blurred_rounded_rect(
                Affine::IDENTITY,
                rect(rng),
                color(rng),
                rng.range_f64(0.0, 16.0),
                rng.range_f64(0.0, 16.0),
            ),
            5 => p.draw_glyphs(
                &font,
                super::num::to_f32(rng.range_f64(8.0, 48.0)).unwrap_or(0.0),
                &[],
                &peniko::Brush::Solid(color(rng)),
                Affine::IDENTITY,
                None,
                Fill::NonZero,
                std::iter::once(Glyph {
                    id: rng.range_u32(33, 127),
                    x: super::num::to_f32(rng.range_f64(0.0, 256.0)).unwrap_or(0.0),
                    y: super::num::to_f32(rng.range_f64(0.0, 256.0)).unwrap_or(0.0),
                }),
            ),
            _ => p.pop_layer(),
        }
    }
    scene
}

#[test]
fn cmd_ops_are_non_decreasing() {
    for_seeds(500, |seed, rng| {
        let scene = random_scene(rng);
        let mut scratch = PrepareScratch::default();
        let input = PrepareInput {
            target_size: (256, 256),
            root_transform: Affine::IDENTITY,
            vertex_limit: u32::MAX,
        };
        prepare(
            &scene,
            &input,
            &crate::test_support::webgl2_limits(),
            &mut scratch,
            &mut crate::stats::FrameStats::default(),
        );
        assert!(
            scratch.plan.cmd_ops.windows(2).all(|w| w[0] <= w[1]),
            "seed {seed}: painter's order violated in cmd_ops"
        );
    });
}
