#![cfg(not(target_family = "wasm"))]

//! Headless regression checks for the wgpu glyph pipeline:
//!   1. glyphs land on one baseline (not scattered) — catches the `placement.top`
//!      sign error,
//!   2. the bundled families (default + monospace) and variable-font weights render.
//!
//! `#[ignore]` because they need a GPU (run with `-- --ignored`).

mod support;

use pebbles_core::IntoWidget;
use pebbles_foundation::palette;
use pebbles_shell::capture;
use pebbles_widgets::{Text, text};
use support::{capture_or_skip, luma, px};

#[test]
#[ignore = "gpu"]
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
#[ignore = "gpu"]
fn bundled_families_and_weights_render() {
    let default: Text = text("Hello").size(32.0).color(palette::BLACK);
    let mono = text("Hello").size(32.0).color(palette::BLACK).font_family("JetBrains Mono");
    let bold = text("Hello").size(32.0).color(palette::BLACK).bold();
    // Space Grotesk has no italic face, so `italic` triggers synthesis (the FAKE_ITALIC skew).
    let synth_italic = text("Hello").size(32.0).color(palette::BLACK).font_family("Space Grotesk").italic();
    for (label, root) in
        [("Inter", default), ("JetBrains Mono", mono), ("bold", bold), ("synthesized italic", synth_italic)]
    {
        let Some(rgba) = capture_or_skip(label, root.into_widget(), 300, 80) else { return };
        // A 32 px line sits in the top ~45 px band: it must hold solid ink, and the far
        // corner stays untouched background.
        let darkest = (0..45)
            .flat_map(|y| (0..300).map(move |x| (x, y)))
            .map(|(x, y)| luma(px(&rgba, 300, x, y)))
            .min();
        assert!(darkest.is_some_and(|l| l <= 80), "{label}: no solid ink, darkest luma {darkest:?}");
        assert_eq!(px(&rgba, 300, 298, 78), [255, 255, 255, 255], "{label}: corner");
    }
}

#[test]
#[ignore = "gpu"]
fn color_emoji_renders() {
    // The committed CBDT test font (V22), registered before capture builds its TextEnv — no
    // system emoji font involved.
    register_emoji_font();
    let root = text("😀").font_family("Noto Color Emoji").size(48.0).into_widget();
    let (w, h) = (120u32, 120u32);
    let Some(rgba) = capture_or_skip("color_emoji_renders", root, w, h) else { return };
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

fn register_emoji_font() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        pebbles_render::fonts::register_user_font(include_bytes!(
            "../../../tools/render-check/fonts/NotoColorEmoji-test.ttf"
        ));
    });
}

/// Rows of `rgba` (`w` wide) holding a pixel with luma ≤ `max`: (first, last).
fn ink_rows(rgba: &[u8], w: u32, h: u32, max: u32) -> Option<(u32, u32)> {
    let rows: Vec<u32> = (0..h).filter(|&y| (0..w).any(|x| luma(px(rgba, w, x, y)) <= max)).collect();
    Some((*rows.first()?, *rows.last()?))
}

/// T23: cosmic-shaped text renders on whichever backend this build selects (run the file under
/// both the default and `--no-default-features --features wgpu`).
#[test]
#[ignore = "gpu"]
fn cosmic_text_renders_on_every_backend() {
    let (w, h) = (300u32, 80u32);
    let root = text("Hello cosmic").size(24.0).color(palette::BLACK).into_widget();
    let Some(rgba) = capture_or_skip("cosmic_text_renders_on_every_backend", root, w, h) else { return };
    let (top, bottom) = ink_rows(&rgba, w, h, 128).expect("ink");
    let darkest =
        (top..=bottom).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| luma(px(&rgba, w, x, y))).min();
    assert!(darkest.is_some_and(|l| l <= 80), "darkest luma {darkest:?}");
    let band = bottom - top + 1;
    assert!((14..=30).contains(&band), "ink band {top}..{bottom} = {band} px");
    assert_eq!(px(&rgba, w, w - 2, h - 2), [255, 255, 255, 255], "background");
}

/// T23: weight and slant reach the renderer — bold darkens the regular run's ink box.
#[test]
#[ignore = "gpu"]
fn bold_and_italic_differ() {
    let (w, h) = (200u32, 60u32);
    let cap = |t: Text| capture_or_skip("bold_and_italic_differ", t.into_widget(), w, h);
    let base = || text("Hello").size(32.0).color(palette::BLACK);
    let (Some(regular), Some(bold), Some(italic)) = (cap(base()), cap(base().bold()), cap(base().italic()))
    else {
        return;
    };
    assert_ne!(regular, bold);
    assert_ne!(regular, italic);
    assert_ne!(bold, italic);
    // Mean luma over the regular run's ink box: bold puts more ink there.
    let (top, bottom) = ink_rows(&regular, w, h, 200).expect("ink");
    let cols: Vec<u32> =
        (0..w).filter(|&x| (top..=bottom).any(|y| luma(px(&regular, w, x, y)) <= 200)).collect();
    let (left, right) = (cols[0], *cols.last().unwrap());
    let mean = |img: &[u8]| {
        let n = f64::from((right - left + 1) * (bottom - top + 1));
        (top..=bottom)
            .flat_map(|y| (left..=right).map(move |x| (x, y)))
            .map(|(x, y)| f64::from(luma(px(img, w, x, y))))
            .sum::<f64>()
            / n
    };
    assert!(mean(&bold) < mean(&regular), "bold {} vs regular {}", mean(&bold), mean(&regular));
}
