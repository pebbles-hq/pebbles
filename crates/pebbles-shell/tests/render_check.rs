#![cfg(all(not(target_family = "wasm"), any(feature = "vello-hybrid", feature = "wgpu")))]

//! Headless regression checks for the wgpu image + shadow pipelines (Phase 3).
//! `#[ignore]` because they need a GPU (run with `-- --ignored`).

use pebbles_core::IntoWidget;
use pebbles_foundation::{Color, Offset, palette};
use pebbles_render::{BoxDecoration, BoxShadow, image_from_rgba8};
use pebbles_shell::capture;
use pebbles_widgets::container;

fn count_where(rgba: &[u8], w: u32, h: u32, f: impl Fn(u8, u8, u8) -> bool) -> u32 {
    let mut count = 0u32;
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if f(rgba[i], rgba[i + 1], rgba[i + 2]) {
                count += 1;
            }
        }
    }
    count
}

#[test]
#[ignore = "requires a GPU"]
fn image_renders() {
    let img = image_from_rgba8(10, 10, [255u8, 0, 0, 255].repeat(100));
    let root =
        container().width(100.0).height(100.0).decoration(BoxDecoration::new().image(img)).into_widget();
    let rgba = capture::capture_rgba(root, 120, 120, palette::WHITE).expect("capture");
    let red = count_where(&rgba, 120, 120, |r, g, b| r > 200 && g < 60 && b < 60);
    assert!(red > 1000, "expected red image pixels, got {red}");
}

#[test]
#[ignore = "requires a GPU"]
fn elevation_shadow_renders() {
    // A white 80×80 box with a strong black blur shadow, on white — the shadow must
    // extend past the box (blur + offset) but NOT cover the box's own center (D01/C07).
    let root = container()
        .width(80.0)
        .height(80.0)
        .color(palette::WHITE)
        .shadow(BoxShadow::new(Color::from_rgba8(0, 0, 0, 200), Offset::new(0.0, 4.0), 16.0, 0.0))
        .into_widget();
    let rgba = capture::capture_rgba(root, 160, 160, palette::WHITE).expect("capture");
    // The box center (40,40) must be pure white — the shadow does not cover the card.
    let i = ((40 * 160 + 40) * 4) as usize;
    let c = [rgba[i], rgba[i + 1], rgba[i + 2]];
    assert!(
        (c[0] as i32 - 255).abs() <= 1 && (c[1] as i32 - 255).abs() <= 1 && (c[2] as i32 - 255).abs() <= 1,
        "box center covered by shadow: {c:?}"
    );
    // Ink outside the box (x > 88) must be the blurred shadow.
    let outside = count_where(&rgba, 160, 160, |r, g, b| r < 240 || g < 240 || b < 240);
    assert!(outside > 100, "expected shadow pixels, got {outside}");
}
