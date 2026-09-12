//! Offscreen capture: render a widget to PNG headlessly and verify it decodes to the
//! requested size. `#[ignore]` because it needs a GPU (run locally with
//! `cargo test -p pebbles-shell --test capture -- --ignored`).

#![cfg(all(not(target_family = "wasm"), feature = "vello-hybrid"))]

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
