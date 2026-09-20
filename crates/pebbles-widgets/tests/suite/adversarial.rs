//! Break-the-app / adversarial layout inputs: the app must **never panic** on
//! hostile geometry. Each case mounts the widget and runs a full settled frame
//! (layout + paint) — a panic anywhere fails the test. This is the framework's own
//! version of `pebbles-charts/tests/robustness.rs`, which does exactly this for the
//! chart family (§4 of the testing checklist).
//!
//! Two families live here:
//!   1. Pathological `BoxConstraints` / window sizes / empty containers.
//!   2. The specific widget-config panics being fixed this cycle (empty breadcrumb,
//!      0-slide carousel autoplay, inverted scale/size bounds, mismatched panel
//!      sizes, `Toggle::default()`, out-of-range date month, bad grid spans) — each
//!      is mounted and drawn to lock the fix with a regression.

use pebbles_core::{AnyWidget, IntoWidget};
use pebbles_render::BoxConstraints;
use pebbles_testing::Harness;
use pebbles_widgets::{
    GridView, Toggle, breadcrumb, carousel, column, constrained_box, container, date_field,
    draggable_scrollable_sheet, interactive_viewer, resizable, row, slider, text,
};

/// Mount `root`, run a settled frame, and return the harness — a panic in layout or
/// paint fails the test right here.
fn mount_draw<W: IntoWidget + 'static>(root: fn() -> W) -> Harness {
    let mut h = Harness::new();
    h.mount(root);
    h.draw();
    h
}

// ---------------------------------------------------------------------------
// 1. Hostile BoxConstraints, injected directly via `constrained_box`.
// ---------------------------------------------------------------------------

fn bc(min_w: f64, max_w: f64, min_h: f64, max_h: f64) -> BoxConstraints {
    BoxConstraints { min_width: min_w, max_width: max_w, min_height: min_h, max_height: max_h }
}

fn nan_constraints() -> impl IntoWidget {
    constrained_box(bc(f64::NAN, f64::NAN, f64::NAN, f64::NAN), text("x"))
}

fn infinite_constraints() -> impl IntoWidget {
    constrained_box(bc(f64::INFINITY, f64::INFINITY, f64::INFINITY, f64::INFINITY), text("x"))
}

fn negative_constraints() -> impl IntoWidget {
    constrained_box(bc(-100.0, -50.0, -100.0, -50.0), text("x"))
}

fn min_exceeds_max_constraints() -> impl IntoWidget {
    // min > max on both axes — the "impossible" box.
    constrained_box(bc(500.0, 100.0, 500.0, 100.0), text("x"))
}

// One test per pathological input, so a partial hardening flips them green
// individually (and a failure names the exact input).

// `BoxConstraints::constrain_width/height` now route through `clamp_range` (constraints.rs),
// which treats NaN bounds as unbounded and orders min/max, so NaN constraints no longer panic.
#[test]
fn nan_constraints_never_panic() {
    mount_draw(nan_constraints);
}

#[test]
fn infinite_constraints_never_panic() {
    mount_draw(infinite_constraints);
}

#[test]
fn negative_constraints_never_panic() {
    mount_draw(negative_constraints);
}

#[test]
fn min_exceeds_max_constraints_never_panic() {
    mount_draw(min_exceeds_max_constraints);
}

// ---------------------------------------------------------------------------
// Degenerate windows — a 0×0 viewport plus NaN / infinite / negative windows.
// ---------------------------------------------------------------------------

fn some_content() -> impl IntoWidget {
    column(vec![text("hello").into_widget(), container().width(120.0).height(40.0).into_widget()])
}

#[test]
fn zero_size_window_never_panics() {
    let mut h = Harness::new().window(0.0, 0.0);
    h.mount(some_content);
    h.draw();
}

fn draw_in_window(w: f64, hgt: f64) {
    let mut h = Harness::new().window(w, hgt);
    h.mount(some_content);
    h.draw();
}

// `Ui::layout` now sanitizes the window size to a finite, non-negative extent before it
// reaches `BoxConstraints::tight`, so a NaN window no longer panics.
#[test]
fn nan_window_never_panics() {
    draw_in_window(f64::NAN, 600.0);
    draw_in_window(800.0, f64::NAN);
}

#[test]
fn infinite_window_never_panics() {
    draw_in_window(f64::INFINITY, 600.0);
    draw_in_window(800.0, f64::INFINITY);
}

// A negative window size is now floored to zero in `Ui::layout`, so it no longer produces
// an inverted `clamp(x, 0.0, negative)`.
#[test]
fn negative_window_never_panics() {
    draw_in_window(-800.0, -600.0);
}

// ---------------------------------------------------------------------------
// Empty-children containers.
// ---------------------------------------------------------------------------

fn empty_column() -> impl IntoWidget {
    column(Vec::<AnyWidget>::new())
}

fn empty_row() -> impl IntoWidget {
    row(Vec::<AnyWidget>::new())
}

fn empty_nested() -> impl IntoWidget {
    column(vec![row(Vec::<AnyWidget>::new()).into_widget(), column(Vec::<AnyWidget>::new()).into_widget()])
}

#[test]
fn empty_containers_never_panic() {
    mount_draw(empty_column);
    mount_draw(empty_row);
    mount_draw(empty_nested);
}

// ---------------------------------------------------------------------------
// 2. Widget-config panics being fixed this cycle — mount + draw, no panic.
// ---------------------------------------------------------------------------

fn empty_breadcrumb() -> impl IntoWidget {
    breadcrumb(Vec::new())
}

fn carousel_autoplay_no_slides() -> impl IntoWidget {
    // Zero slides + autoplay: the advance math must not divide by the page count.
    carousel(Vec::<AnyWidget>::new()).autoplay(1.0)
}

fn interactive_viewer_inverted_scale() -> impl IntoWidget {
    interactive_viewer(text("zoom")).min_scale(5.0).max_scale(1.0)
}

fn resizable_sizes_shorter_than_panels() -> impl IntoWidget {
    // Three panels, one size — the drag path must not index out of bounds.
    resizable(vec![
        container().color(pebbles_foundation::palette::BLUE).into_widget(),
        container().color(pebbles_foundation::palette::RED).into_widget(),
        container().color(pebbles_foundation::palette::GREEN).into_widget(),
    ])
    .sizes(vec![100.0])
}

fn toggle_default() -> impl IntoWidget {
    // `Toggle::default()` has no child.
    Toggle::default()
}

fn slider_min_exceeds_max() -> impl IntoWidget {
    slider(200.0).min(100.0).max(0.0)
}

fn draggable_sheet_min_exceeds_max() -> impl IntoWidget {
    // min > max fraction — the fraction clamp must not `f64::clamp` with min > max.
    draggable_scrollable_sheet(column(vec![text("body").into_widget()])).min(0.9).max(0.1)
}

fn gridview_zero_spans() -> impl IntoWidget {
    container()
        .height(200.0)
        .child(GridView::builder(12, 3, 60.0, |i| text(format!("g{i}"))).spans(|_| (0, 0)))
}

fn gridview_oversized_spans() -> impl IntoWidget {
    // A span far wider than the column count.
    container()
        .height(200.0)
        .child(GridView::builder(12, 3, 60.0, |i| text(format!("g{i}"))).spans(|_| (99, 99)))
}

fn date_field_month_over_12() -> impl IntoWidget {
    // Out-of-range months on both the bounds and the seeded range value.
    date_field().range(true).range_value((2024, 13, 5), (2025, 0, 40)).min(2024, 13, 1).max(2024, 99, 1)
}

#[test]
fn widget_edge_configs_never_panic() {
    mount_draw(empty_breadcrumb);
    mount_draw(interactive_viewer_inverted_scale);
    mount_draw(resizable_sizes_shorter_than_panels);
    mount_draw(toggle_default);
    mount_draw(slider_min_exceeds_max);
    mount_draw(draggable_sheet_min_exceeds_max);
    mount_draw(gridview_zero_spans);
    mount_draw(gridview_oversized_spans);
    mount_draw(date_field_month_over_12);
}

#[test]
fn carousel_autoplay_with_no_slides_never_panics() {
    // Mount, then let the autoplay interval attempt an advance across a few ticks —
    // the empty-page advance is where a modulo-by-zero would fire.
    let mut h = Harness::new();
    h.mount(carousel_autoplay_no_slides);
    h.draw();
    h.tick(1.0);
    h.tick(1.0);
}

// ---------------------------------------------------------------------------
// 3. ImageView decode-failure path — garbage / truncated / empty / bad base64
//    must yield the error widget, never panic. The decode is synchronous for the
//    `memory` / `base64` sources, so a settled frame is enough to exercise it.
//    `ImageView` is behind the `image-view` feature (on in CI's `--all-features`).
// ---------------------------------------------------------------------------

#[cfg(feature = "image-view")]
mod image_decode {
    use pebbles_core::IntoWidget;
    use pebbles_render::RenderParagraph;
    use pebbles_testing::Harness;
    use pebbles_widgets::ImageView;

    // The `memory`/`base64` sources decode synchronously, so a settled frame goes
    // straight to the error branch (`Source::Ready(Err(_)) => error_box`). Each error
    // box is built from the DEFAULT error widget, which paints a "⚠" — a
    // `RenderParagraph` in the tree — so its presence proves the error branch rendered
    // (a successfully decoded image paints a decorated box, no paragraph).

    fn img_garbage() -> pebbles_core::AnyWidget {
        ImageView::memory([0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x11, 0x22]).width(80.0).height(80.0).into_widget()
    }

    fn img_truncated_png() -> pebbles_core::AnyWidget {
        // A valid PNG signature followed by nothing — a truncated file.
        ImageView::memory([0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A])
            .width(80.0)
            .height(80.0)
            .into_widget()
    }

    fn img_empty_bytes() -> pebbles_core::AnyWidget {
        ImageView::memory(Vec::<u8>::new()).width(80.0).height(80.0).into_widget()
    }

    fn img_bad_base64() -> pebbles_core::AnyWidget {
        ImageView::base64("data:image/png;base64,!!!not-valid-base64!!!")
            .width(80.0)
            .height(80.0)
            .into_widget()
    }

    #[test]
    fn image_view_decode_failures_render_error_not_panic() {
        let builders: [fn() -> pebbles_core::AnyWidget; 4] =
            [img_garbage, img_truncated_png, img_empty_bytes, img_bad_base64];
        for builder in builders {
            let mut h = Harness::new();
            h.mount(builder);
            h.draw();
            assert!(
                h.find::<RenderParagraph>().is_some(),
                "a decode failure must render the ⚠ error box, never panic"
            );
        }
    }
}
