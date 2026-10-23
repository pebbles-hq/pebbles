//! GPU soak tests (10 §9.2, M05/M20): leaks are judged by `GpuCensus`, never RSS.
#![cfg(debug_assertions)]

use pebbles_gpu::OffscreenRenderer;
use pebbles_render::paint::Scene;
use peniko::Color;

/// M05: a fresh renderer after one empty frame holds no glyphs, images or gradient rows, and
/// exactly one frame-target entry.
#[test]
#[ignore = "gpu"]
fn census_of_fresh_renderer_is_minimal() {
    let Ok(mut o) = OffscreenRenderer::new() else {
        eprintln!("census_of_fresh_renderer_is_minimal: no adapter, skipped");
        return;
    };
    o.render_rgba(&Scene::new(), (16, 16), Color::WHITE, 1.0).expect("empty frame renders");
    let c = o.census();
    assert_eq!(c.glyph_entries, 0, "{c:?}");
    assert_eq!(c.image_entries, 0, "{c:?}");
    assert_eq!(c.gradient_rows_used, 0, "{c:?}");
    assert_eq!(c.frame_target_entries, 1, "{c:?}");
}

/// One soak round: text in 6 sizes × every bundled family, a grid of 32 images created and
/// dropped within the round, layers and gradients, each at 5 target sizes.
fn soak_round(o: &mut OffscreenRenderer) {
    use kurbo::{Affine, Rect};
    use pebbles_render::paint::{Glyph, Painter};
    use peniko::{Brush, Fill, Gradient, Mix};

    // The bundled families are loaded once, as the app's TextEnv does. (Fresh blobs every round
    // would be new fonts to the glyph cache; their entries are only reclaimed by eviction.)
    static FONTS: std::sync::OnceLock<Vec<peniko::FontData>> = std::sync::OnceLock::new();
    let fonts = FONTS.get_or_init(|| {
        pebbles_render::fonts::builtin_fonts()
            .iter()
            .map(|(_, bytes)| peniko::FontData::new(peniko::Blob::from(bytes.to_vec()), 0))
            .collect()
    });
    let images: Vec<peniko::ImageData> = (0..32u8)
        .map(|i| peniko::ImageData {
            data: peniko::Blob::from([i * 7, 90, 200, 255].repeat(64)),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::Alpha,
            width: 8,
            height: 8,
        })
        .collect();
    for size in [(64, 48), (128, 96), (200, 150), (320, 240), (400, 300)] {
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        for (f, font) in fonts.iter().enumerate() {
            for (k, px) in [9.0f32, 11.0, 13.0, 16.0, 20.0, 28.0].into_iter().enumerate() {
                let y = 10.0 + (f * 6 + k) as f32 * 4.0;
                let glyphs = (0..10u32).map(|i| Glyph { id: 36 + i, x: 2.0 + i as f32 * px * 0.6, y });
                p.draw_glyphs(
                    font,
                    px,
                    &[],
                    &Brush::Solid(Color::BLACK),
                    Affine::IDENTITY,
                    None,
                    Fill::NonZero,
                    glyphs,
                );
            }
        }
        for (i, img) in images.iter().enumerate() {
            let (x, y) = ((i % 8) as f64 * 9.0, 100.0 + (i / 8) as f64 * 9.0);
            p.draw_image(&peniko::ImageBrush::new(img.clone()), Affine::translate((x, y)));
        }
        let clip = Rect::new(150.0, 20.0, 250.0, 120.0);
        p.push_layer(Fill::NonZero, Mix::Normal, 0.6, Affine::IDENTITY, &clip);
        let g = Gradient::new_linear((150.0, 0.0), (250.0, 0.0))
            .with_stops([Color::BLACK, Color::WHITE].as_slice());
        p.fill(Fill::NonZero, Affine::IDENTITY, &g, None, &clip);
        p.pop_layer();
        o.render_rgba(&s, size, Color::WHITE, 1.0).expect("soak frame renders");
    }
}

/// M20 (B15): the census after rounds 2 and 3 equals the census after round 1. Images are
/// created and dropped inside each round, so a cache that never forgets shows up here.
#[test]
#[ignore = "gpu"]
fn census_flat_across_rounds() {
    let Ok(mut o) = OffscreenRenderer::new() else {
        eprintln!("census_flat_across_rounds: no adapter, skipped");
        return;
    };
    soak_round(&mut o);
    let baseline = o.census();
    for round in 2..=3 {
        soak_round(&mut o);
        let now = o.census();
        let drift: Vec<String> = pebbles_gpu::GpuCensus::COLUMNS
            .iter()
            .zip(baseline.values().into_iter().zip(now.values()))
            .filter(|(_, (a, b))| a != b)
            .map(|(name, (a, b))| format!("{name}: {a} → {b} ({:+})", b as i128 - a as i128))
            .collect();
        assert!(drift.is_empty(), "round {round} drifted from round 1:\n{}", drift.join("\n"));
    }
}
