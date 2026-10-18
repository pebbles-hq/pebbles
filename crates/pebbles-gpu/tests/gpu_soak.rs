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
