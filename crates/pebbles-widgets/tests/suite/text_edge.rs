//! Text shaping on hostile input: a right-to-left script, an emoji / ZWJ grapheme
//! cluster, and a very large single paragraph must all lay out + paint through the
//! parley path without panicking, and produce non-zero laid-out bounds. The
//! assertion is deliberately "shapes, sizes, paints" — the point is exercising the
//! shaping path on inputs that trip naive byte/char handling, not pixel-checking
//! the glyphs.

use pebbles_core::{IntoWidget, component};
use pebbles_render::RenderParagraph;
use pebbles_testing::Harness;
use pebbles_widgets::{center, text};

/// Draw the mounted tree and assert its paragraph shaped to non-zero bounds.
fn assert_shapes_nonzero(h: &Harness) {
    let id = h.find::<RenderParagraph>().expect("the text rendered a paragraph");
    let size = h.size_of(id);
    assert!(size.width > 0.0 && size.height > 0.0, "shaped paragraph has non-zero bounds: {size:?}");
}

// ---------------------------------------------------------------------------
// (a) RTL script — Arabic
// ---------------------------------------------------------------------------

fn arabic() -> impl IntoWidget {
    // "مرحبا بالعالم" — Arabic for "hello world": an RTL run with contextual joining.
    center(text("مرحبا بالعالم").size(18.0))
}

#[test]
fn rtl_arabic_shapes_without_panic() {
    let mut h = Harness::new();
    h.mount(|| component(arabic));
    h.draw();
    assert_shapes_nonzero(&h);
}

// ---------------------------------------------------------------------------
// (b) emoji + ZWJ grapheme clusters
// ---------------------------------------------------------------------------

fn emoji() -> impl IntoWidget {
    // A ZWJ family sequence, a regional-indicator flag, and a skin-tone modifier —
    // each is one grapheme spanning several code points.
    center(text("👨‍👩‍👧‍👦 🇯🇵 👍🏽").size(20.0))
}

#[test]
fn emoji_zwj_grapheme_clusters_shape_without_panic() {
    let mut h = Harness::new();
    h.mount(|| component(emoji));
    h.draw();
    assert_shapes_nonzero(&h);
}

// ---------------------------------------------------------------------------
// (c) a very large single paragraph
// ---------------------------------------------------------------------------

fn huge() -> impl IntoWidget {
    // ≥100k chars in one paragraph (with spaces, so it wraps rather than forming one
    // impossibly long line). Must shape + lay out + paint without panicking.
    center(text("Pebbles ".repeat(14_000)).size(12.0))
}

#[test]
fn very_large_paragraph_lays_out_within_bounds() {
    let mut h = Harness::new().window(600.0, 400.0);
    h.mount(|| component(huge));
    h.draw();
    assert_shapes_nonzero(&h);
    // It wrapped into the viewport width rather than exploding past it.
    let id = h.find::<RenderParagraph>().expect("paragraph");
    assert!(h.size_of(id).width <= 600.0 + 0.5, "the paragraph wrapped within the window width");
}
