//! Wheel axis-fallback (`dispatch_scroll`): the mouse wheel arrives as a single
//! vertical delta, so `dispatch_scroll` prefers a vertical scrollable and bubbles
//! PAST an axis-mismatched one — but falls back to any axis when a horizontal list
//! is the only scrollable under the pointer.
//!
//! * (a) A lone horizontal viewport still wheels (the any-axis fallback catches it).
//! * (b) A wheel over a horizontal strip embedded in a vertical page scrolls the
//!   PAGE, not the strip (the vertical pass bubbles past the mismatched inner).

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Axis, MainAxisSize, Offset, Size, palette};
use pebbles_render::{RenderScroll, TextEnv};
use pebbles_widgets::{SingleChildScrollView, View, column, container, scroll_view};

/// The `(offset, target)` of the scroll view on `axis` (panics if there is none).
fn scroll_axis(ui: &Ui, axis: Axis) -> (f64, f64) {
    let tree = ui.render_tree();
    for id in tree.find_all::<RenderScroll>() {
        let s = tree.object_ref(id).downcast_ref::<RenderScroll>().unwrap();
        if s.axis == axis {
            return (s.offset, s.target);
        }
    }
    panic!("no {axis:?} scroll view in the tree");
}

// ---------------------------------------------------------------------------
// (a) a lone horizontal list wheels via the any-axis fallback
// ---------------------------------------------------------------------------

fn horizontal_only() -> impl IntoWidget {
    // Content far wider than the viewport → scrollable on X only.
    SingleChildScrollView::horizontal(container().width(3000.0).height(60.0).color(palette::BLUE))
}

#[test]
fn wheel_scrolls_a_lone_horizontal_list() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(horizontal_only)).into_widget());
    ui.layout(&mut env, Size::new(300.0, 200.0));

    // Only a horizontal viewport exists.
    assert_eq!(ui.render_tree().find_all::<RenderScroll>().len(), 1, "one horizontal viewport");
    let (off0, _) = scroll_axis(&ui, Axis::Horizontal);
    assert!(off0.abs() < 1e-6, "starts at the left edge");

    // The vertical pass bubbles past the axis-mismatched horizontal view; the any-axis
    // fallback then delivers the wheel to it.
    assert!(ui.dispatch_scroll(Offset::new(150.0, 100.0), 120.0), "the wheel moved the horizontal list");
    let (_, target) = scroll_axis(&ui, Axis::Horizontal);
    assert!(target > 0.0, "the horizontal scroll target advanced: {target}");
}

// ---------------------------------------------------------------------------
// (b) a wheel over a horizontal strip inside a vertical page bubbles to the page
// ---------------------------------------------------------------------------

fn vertical_page_with_horizontal_strip() -> impl IntoWidget {
    // A horizontal strip at the top of a page whose body overflows vertically.
    let strip = SingleChildScrollView::horizontal(container().width(3000.0).height(80.0).color(palette::RED));
    scroll_view(
        column(vec![
            container().height(80.0).child(strip).into_widget(),
            container().width(240.0).height(2000.0).color(palette::BLUE).into_widget(),
        ])
        .main_axis_size(MainAxisSize::Min),
    )
}

#[test]
fn wheel_over_axis_mismatched_inner_bubbles_to_the_page() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(vertical_page_with_horizontal_strip)).into_widget());
    ui.layout(&mut env, Size::new(300.0, 200.0));

    // Two viewports: the vertical page and the horizontal strip.
    assert_eq!(ui.render_tree().find_all::<RenderScroll>().len(), 2, "page + horizontal strip");

    // A vertical wheel delivered over the strip (window y 0..80) must scroll the PAGE
    // (matching axis) and leave the strip untouched (mismatched axis → bubbled past).
    assert!(ui.dispatch_scroll(Offset::new(150.0, 40.0), 120.0), "the wheel moved a scrollable");
    let (_, page_target) = scroll_axis(&ui, Axis::Vertical);
    let (strip_off, strip_target) = scroll_axis(&ui, Axis::Horizontal);
    assert!(page_target > 0.0, "the vertical page took the wheel: {page_target}");
    assert!(
        strip_off.abs() < 1e-6 && strip_target.abs() < 1e-6,
        "the horizontal strip was NOT scrolled: off={strip_off} target={strip_target}"
    );
}
