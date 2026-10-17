//! Offscreen capture: render a widget to PNG headlessly and verify it decodes to the
//! requested size. `#[ignore]` because it needs a GPU (run locally with
//! `cargo test -p pebbles-shell --test capture -- --ignored`).

#![cfg(all(not(target_family = "wasm"), any(feature = "vello-hybrid", feature = "wgpu")))]

use pebbles_core::IntoWidget;
use pebbles_foundation::{Color, palette};
use pebbles_shell::capture;
use pebbles_widgets::{container, text};

#[test]
#[ignore = "requires a GPU"]
fn capture_png_produces_a_decodable_image() {
    let root = container()
        .color(Color::from_rgba8(0x63, 0x66, 0xF1, 0xFF))
        .child(text("hello").color(palette::WHITE))
        .into_widget();

    let png = capture::capture_png(root, 120, 80, palette::WHITE).expect("capture");
    assert!(png.len() > 8);
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "PNG magic header");

    let decoder = png::Decoder::new(std::io::Cursor::new(&png));
    let reader = decoder.read_info().expect("decode header");
    let info = reader.info();
    assert_eq!((info.width, info.height), (120, 80));
}

#[test]
#[ignore = "requires a GPU"]
fn offscreen_capture_reuses_one_device_across_frames() {
    let mut gpu = capture::OffscreenCapture::new().expect("device");
    let red = Color::from_rgba8(255, 0, 0, 255);
    let a = gpu.render_scene(&pebbles_render::Scene::new(), 64, 32, red).expect("frame 1");
    assert_eq!(a.len(), 64 * 32 * 4);
    assert!(a.as_chunks::<4>().0.iter().all(|p| *p == [255, 0, 0, 255]));
    let green = Color::from_rgba8(0, 255, 0, 255);
    let b = gpu.render_scene(&pebbles_render::Scene::new(), 16, 8, green).expect("frame 2");
    assert_eq!(b.len(), 16 * 8 * 4);
    assert!(b.as_chunks::<4>().0.iter().all(|p| *p == [0, 255, 0, 255]));
}

#[test]
#[ignore = "requires a GPU"]
fn offscreen_capture_rejects_zero_and_oversized_frames() {
    let mut gpu = capture::OffscreenCapture::new().expect("device");
    let s = pebbles_render::Scene::new();
    assert!(gpu.render_scene(&s, 0, 10, palette::WHITE).is_err());
    assert!(gpu.render_scene(&s, capture::MAX_CAPTURE_DIM + 1, 1, palette::WHITE).is_err());
}
