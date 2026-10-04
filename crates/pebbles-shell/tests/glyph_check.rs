//! Headless regression checks for the wgpu glyph pipeline:
//!   1. glyphs land on one baseline (not scattered) — catches the `placement.top`
//!      sign error,
//!   2. the bundled families (default + monospace) and variable-font weights render.
//!
//! `#[ignore]` because they need a GPU (run with `-- --ignored`).

use pebbles_core::IntoWidget;
use pebbles_foundation::palette;
use pebbles_shell::capture;
use pebbles_widgets::{Text, text};

/// Count pixels darker than a near-white threshold (i.e. "ink").
fn ink_pixels(rgba: &[u8], w: u32, h: u32) -> u32 {
    let mut count = 0u32;
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if rgba[i] < 200 || rgba[i + 1] < 200 || rgba[i + 2] < 200 {
                count += 1;
            }
        }
    }
    count
}

#[test]
#[ignore = "requires a GPU"]
fn glyphs_share_one_baseline() {
    let root = text("Hello World").color(palette::BLACK).size(40.0).into_widget();
    let w = 400u32;
    let h = 100u32;
    let rgba = capture::capture_rgba(root, w, h, palette::WHITE).expect("capture");

    let (mut min_y, mut max_y, mut count) = (u32::MAX, 0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if rgba[i] < 200 || rgba[i + 1] < 200 || rgba[i + 2] < 200 {
                count += 1;
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    assert!(count > 100, "expected a non-trivial amount of text pixels, got {count}");
    // A single 40px line must fit within a ~60px band; scattered glyphs would span
    // nearly the full 100px frame.
    assert!(max_y - min_y < 60, "glyph band too tall: {}..{}", min_y, max_y);
}

#[test]
#[ignore = "requires a GPU"]
fn bundled_families_and_weights_render() {
    let default: Text = text("Hello").size(32.0).color(palette::BLACK);
    let mono = text("Hello").size(32.0).color(palette::BLACK).font_family("JetBrains Mono");
    let bold = text("Hello").size(32.0).color(palette::BLACK).bold();
    // Space Grotesk has no italic face, so `italic` triggers synthesis (the FAKE_ITALIC skew).
    let synth_italic = text("Hello").size(32.0).color(palette::BLACK).font_family("Space Grotesk").italic();
    for (label, root) in
        [("Inter", default), ("JetBrains Mono", mono), ("bold", bold), ("synthesized italic", synth_italic)]
    {
        let rgba = capture::capture_rgba(root.into_widget(), 300, 80, palette::WHITE).expect("capture");
        let ink = ink_pixels(&rgba, 300, 80);
        assert!(ink > 50, "{label}: expected glyphs to render, got {ink} ink pixels");
    }
}

#[test]
#[ignore = "requires a GPU and a system color-emoji font"]
fn color_emoji_renders() {
    let root = text("😀").size(48.0).into_widget();
    let (w, h) = (120u32, 120u32);
    let rgba = capture::capture_rgba(root, w, h, palette::WHITE).expect("capture");
    let mut ink = 0u32;
    let mut colored = 0u32;
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let (r, g, b) = (rgba[i] as i32, rgba[i + 1] as i32, rgba[i + 2] as i32);
            if r < 250 || g < 250 || b < 250 {
                ink += 1;
                if (r - g).abs() > 20 || (g - b).abs() > 20 || (r - b).abs() > 20 {
                    colored += 1;
                }
            }
        }
    }
    // The emoji must render as a color bitmap (the Rgba8 atlas + color pipeline),
    // not a monochrome fallback.
    assert!(ink > 20, "expected emoji glyph pixels, got {ink}");
    assert!(colored > 0, "expected colored emoji pixels, got {colored} colored of {ink}");
}
