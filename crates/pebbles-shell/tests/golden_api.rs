//! The public pixel-golden API (05 V54): an app renders a scene and compares it with one
//! committed golden that holds on every OS (DR-21). Needs a GPU (`--ignored`; lavapipe in CI).

#![cfg(not(target_family = "wasm"))]

use kurbo::{Affine, Rect, RoundedRect};
use pebbles_render::Scene;
use pebbles_render::paint::Painter;
use peniko::{Color, Fill};

/// A card with a shadow and a stroked border: shapes only, so the golden needs no fonts.
fn card() -> Scene {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    let card = RoundedRect::new(16.0, 12.0, 112.0, 76.0, 10.0);
    p.draw_blurred_rounded_rect(Affine::IDENTITY, card.rect(), Color::BLACK.with_alpha(0.25), 10.0, 6.0);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgb8(99, 102, 241), None, &card);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &Rect::new(28.0, 28.0, 84.0, 36.0));
    scene
}

/// Matches the committed `tests/goldens/golden_api_card.png` (recorded with
/// `PEBBLES_UPDATE_GOLDENS=1`).
#[test]
#[ignore = "gpu"]
fn golden_roundtrip() {
    pebbles_shell::capture::assert_scene_matches_golden(&card(), (128, 96), 1.0, "golden_api_card");
}
