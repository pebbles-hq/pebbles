//! V01 smoke tests: the adapter the lane expects, and one solid rect through the offscreen API.

use super::support::{adapter_summary, gpu_or_skip, near, pixels_meaningful, px};
use super::*;

#[test]
#[ignore = "gpu"]
fn adapter_matches_expectation() {
    let Some(r) = gpu_or_skip("adapter_matches_expectation") else { return };
    if let Ok(expect) = std::env::var("PEBBLES_EXPECT_ADAPTER") {
        let got = adapter_summary(&r);
        assert!(
            got.to_lowercase().contains(&expect.to_lowercase()),
            "adapter {got:?} does not match {expect:?}"
        );
    }
}

#[test]
#[ignore = "gpu"]
fn solid_rect_center_pixel() {
    let Some(mut r) = gpu_or_skip("solid_rect_center_pixel") else { return };
    let mut scene = Scene::new();
    Painter::new(&mut scene).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0x33, 0x66, 0xFF, 0xFF),
        None,
        &Rect::new(8.0, 8.0, 56.0, 56.0),
    );
    let image = r.render_rgba(&scene, (64, 64), Color::WHITE, 1.0).expect("render");
    if pixels_meaningful(&r) {
        assert!(near(px(&image.pixels, 64, 32, 32), [0x33, 0x66, 0xFF, 0xFF], 1));
        assert_eq!(px(&image.pixels, 64, 2, 2), [255, 255, 255, 255]);
    }
}
