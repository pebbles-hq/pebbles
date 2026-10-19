//! M18: CPU prepare allocates nothing in steady state (10 §1.2 P1, B6). Scenes mirror the
//! render-check `bench/` scenes: a dashboard, 1 000 glyph runs and 64 images.

use kurbo::{Affine, BezPath, Circle, Rect, RoundedRect, Stroke};
use pebbles_render::paint::{Glyph, Painter, Scene};
use peniko::{Brush, Color, Fill};

use super::{PrepareInput, PrepareScratch, prepare};
use crate::stats::FrameStats;
use crate::test_alloc::count_allocs;

fn dashboard() -> Scene {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    let font = crate::test_support::bundled_font();
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(243, 244, 246),
        None,
        &Rect::new(0.0, 0.0, 1200.0, 800.0),
    );
    for row in 0..4 {
        for col in 0..4 {
            let (x, y) = (24.0 + f64::from(col) * 292.0, 24.0 + f64::from(row) * 190.0);
            let card = RoundedRect::new(x, y, x + 276.0, y + 174.0, 12.0);
            p.draw_blurred_rounded_rect(
                Affine::IDENTITY,
                card.rect(),
                Color::from_rgba8(0, 0, 0, 40),
                12.0,
                6.0,
            );
            p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &card);
            p.stroke(&Stroke::new(1.0), Affine::IDENTITY, Color::from_rgb8(229, 231, 235), None, &card);
            let mut chart = BezPath::new();
            chart.move_to((x + 16.0, y + 150.0));
            for k in 1..12 {
                let t = f64::from(k);
                chart.line_to((
                    x + 16.0 + t * 20.0,
                    y + 150.0 - 40.0 * (1.0 + (t * 0.7 + f64::from(row)).sin()),
                ));
            }
            p.stroke(&Stroke::new(2.0), Affine::IDENTITY, Color::from_rgb8(59, 130, 246), None, &chart);
            p.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgb8(16, 185, 129),
                None,
                &Circle::new((x + 250.0, y + 26.0), 8.0),
            );
            let glyphs: Vec<Glyph> =
                (0..14).map(|i| Glyph { id: 36 + i, x: i as f32 * 9.0, y: 0.0 }).collect();
            let brush = Brush::Solid(Color::from_rgb8(17, 24, 39));
            p.draw_glyphs(
                &font,
                15.0,
                &[],
                &brush,
                Affine::translate((x + 16.0, y + 30.0)),
                None,
                Fill::NonZero,
                glyphs.iter().copied(),
            );
        }
    }
    s
}

fn glyph_runs_1k() -> Scene {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    let font = crate::test_support::bundled_font();
    let brush = Brush::Solid(Color::BLACK);
    for i in 0..1000u32 {
        let glyphs = [Glyph { id: 36 + i % 26, x: 0.0, y: 0.0 }, Glyph { id: 40 + i % 20, x: 8.0, y: 0.0 }];
        let xf = Affine::translate((f64::from(i % 40) * 30.0, 14.0 + f64::from(i / 40) * 30.0));
        p.draw_glyphs(&font, 13.0, &[], &brush, xf, None, Fill::NonZero, glyphs.into_iter());
    }
    s
}

fn images_64() -> Scene {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    for i in 0..64u32 {
        let px: Vec<u8> = (0..16 * 16u32)
            .flat_map(|k| {
                [(k * 7 + i).to_le_bytes()[0], (k * 3).to_le_bytes()[0], (i * 4).to_le_bytes()[0], 255]
            })
            .collect();
        let img = peniko::ImageBrush::new(peniko::ImageData {
            data: peniko::Blob::from(px),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::Alpha,
            width: 16,
            height: 16,
        });
        p.draw_image(
            &img,
            Affine::translate((f64::from(i % 8) * 140.0, f64::from(i / 8) * 96.0)) * Affine::scale(4.0),
        );
    }
    s
}

/// Layers, clips, gradients and image brushes (09): the Phase 4 vocabulary.
fn layers_and_paints() -> Scene {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    let g = peniko::Gradient::new_linear((0.0, 0.0), (400.0, 0.0))
        .with_stops([Color::from_rgb8(239, 68, 68), Color::from_rgb8(59, 130, 246)].as_slice());
    for i in 0..8 {
        let x = f64::from(i) * 140.0;
        p.push_layer(
            Fill::NonZero,
            peniko::Mix::Normal,
            0.6,
            Affine::IDENTITY,
            &RoundedRect::new(x, 10.0, x + 120.0, 210.0, 16.0),
        );
        p.fill(Fill::NonZero, Affine::IDENTITY, &g, None, &Rect::new(x, 0.0, x + 130.0, 220.0));
        p.push_layer(
            Fill::NonZero,
            peniko::Mix::Multiply,
            1.0,
            Affine::IDENTITY,
            &Circle::new((x + 60.0, 110.0), 40.0),
        );
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(16, 185, 129),
            None,
            &Rect::new(x, 60.0, x + 130.0, 160.0),
        );
        p.pop_layer();
        p.pop_layer();
    }
    s
}

#[test]
fn prepare_steady_state_allocates_nothing() {
    let input = PrepareInput {
        target_size: (1200, 800),
        root_transform: Affine::IDENTITY,
        vertex_limit: u32::MAX,
        scale_factor: 1.0,
        glyph_hint: None,
    };
    let limits = crate::test_support::webgl2_limits();
    for (name, scene) in [
        ("dashboard_1200x800", dashboard()),
        ("glyph_runs_1k", glyph_runs_1k()),
        ("images_64", images_64()),
        ("layers_and_paints", layers_and_paints()),
    ] {
        let mut scratch = PrepareScratch::default();
        let mut stats = FrameStats::default();
        for _ in 0..3 {
            prepare(&scene, &input, &limits, &mut scratch, &mut stats);
            scratch.plan.release_refs();
        }
        let ((), n) = count_allocs(|| {
            for _ in 0..7 {
                prepare(&scene, &input, &limits, &mut scratch, &mut stats);
                scratch.plan.release_refs();
            }
        });
        assert_eq!(n, 0, "{name}: {n} heap allocations in 7 steady-state prepares");
    }
}
