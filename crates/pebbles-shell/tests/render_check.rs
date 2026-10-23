#![cfg(not(target_family = "wasm"))]

//! Headless regression checks for the image + shadow pipelines, as analytic pixel
//! assertions (05 §4.1). `#[ignore]` because they need a GPU (run with `-- --ignored`).

mod support;

use pebbles_core::IntoWidget;
use pebbles_foundation::{Color, EdgeInsets, Offset, palette};
use pebbles_render::{BoxDecoration, BoxShadow, image_from_rgba8};
use pebbles_widgets::container;
use support::{capture_on_or_skip, capture_or_skip, luma, near, px};

#[test]
#[ignore = "gpu"]
fn image_renders() {
    let img = image_from_rgba8(10, 10, [255u8, 0, 0, 255].repeat(100));
    let root = container()
        .padding(EdgeInsets::all(20.0))
        .child(container().width(100.0).height(100.0).decoration(BoxDecoration::new().image(img)))
        .into_widget();
    let Some(rgba) = capture_or_skip("image_renders", root, 160, 160) else { return };
    // The image covers (20,20)–(120,120); outside it the white background shows.
    assert!(near(px(&rgba, 160, 70, 70), [255, 0, 0, 255], 2), "image center: {:?}", px(&rgba, 160, 70, 70));
    assert_eq!(px(&rgba, 160, 150, 150), [255, 255, 255, 255]);
}

#[test]
#[ignore = "gpu"]
fn elevation_shadow_renders() {
    // A white card with a strong blurred shadow on a light-gray page. In painter's order the
    // shadow is drawn BEHIND the card (DR-04, C07), so the card's center stays pure white.
    let bg = Color::from_rgba8(0xF0, 0xF0, 0xF0, 0xFF);
    let card = container().width(80.0).height(80.0).color(palette::WHITE).shadow(BoxShadow::new(
        Color::from_rgba8(0, 0, 0, 200),
        Offset::new(0.0, 4.0),
        16.0,
        0.0,
    ));
    let root = container().padding(EdgeInsets::all(40.0)).child(card).into_widget();
    let Some(rgba) = capture_on_or_skip("elevation_shadow_renders", root, 200, 200, bg) else { return };
    // The root's tight constraints stretch the padded card to (40,40)–(160,160) (as in Flutter).
    let center = px(&rgba, 200, 100, 100);
    assert!(near(center, [255, 255, 255, 255], 1), "card center covered by its shadow: {center:?}");
    let below = px(&rgba, 200, 100, 166);
    assert!(luma(below) <= 0xF0 - 10, "no shadow below the card: {below:?}");
    let corner = px(&rgba, 200, 196, 4);
    assert!(near(corner, [0xF0, 0xF0, 0xF0, 255], 2), "page corner: {corner:?}");
}
