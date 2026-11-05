//! A reversed `ListView` (Flutter's `reverse`: chat logs, consoles, terminals): it starts
//! scrolled to the END, item 0 is built and sits at the bottom, the wheel walks back
//! toward the high indices, and a short list hugs the bottom.

use pebbles_foundation::Offset;
use pebbles_render::{RenderList, ScrollUnit};
use pebbles_testing::Harness;
use pebbles_widgets::{ListView, SizedBox, text};

fn list(h: &Harness) -> &RenderList {
    h.object::<RenderList>(h.find_all::<RenderList>()[0])
}

#[test]
fn reversed_list_starts_at_the_end_and_builds_the_visible_items() {
    let mut h = Harness::new().window(200.0, 200.0);
    h.mount_widget(SizedBox::exact(
        200.0,
        200.0,
        ListView::builder(500, 20.0, |i| text(format!("row {i}"))).reverse(),
    ));
    h.frames(2);
    let max = 500.0 * 20.0 - 200.0;
    assert_eq!(list(&h).start_offset(), max, "starts scrolled to the bottom");
    assert!(h.find_text("row 0") && h.find_text("row 9"), "the bottom rows are built");
    assert!(!h.find_text("row 400"), "far rows are not built");
    // The wheel scrolls up (negative = toward the start) into the high indices.
    h.wheel(Offset::new(100.0, 100.0), Offset::new(0.0, -100.0), ScrollUnit::Pixels);
    h.settle();
    assert_eq!(list(&h).start_offset(), max - 100.0);
    assert!(h.find_text("row 14"), "five rows further up are built");
    // And back down: pinned at the end, no overshoot.
    h.wheel(Offset::new(100.0, 100.0), Offset::new(0.0, 500.0), ScrollUnit::Pixels);
    h.settle();
    assert_eq!(list(&h).offset, 0.0);
    assert_eq!(list(&h).start_offset(), max);
}

#[test]
fn short_reversed_list_hugs_the_bottom() {
    let mut h = Harness::new().window(200.0, 200.0);
    h.mount_widget(SizedBox::exact(
        200.0,
        200.0,
        ListView::builder(3, 20.0, |i| text(format!("row {i}"))).reverse(),
    ));
    h.frames(2);
    // Content (60) is shorter than the viewport (200): it sits on the bottom edge.
    let tree = h.ui.render_tree();
    let l = h.find_all::<RenderList>()[0];
    let child = tree.children_of(l)[0];
    assert_eq!(tree.offset_of(child).y, 140.0);
}

thread_local! {
    static REVERSE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static SC: std::cell::Cell<Option<pebbles_widgets::ScrollController>> =
        const { std::cell::Cell::new(None) };
}

fn controlled() -> SizedBox {
    let sc = pebbles_widgets::use_scroll_controller();
    SC.with(|s| s.set(Some(sc)));
    let l = ListView::builder(100, 20.0, |i| text(format!("row {i}"))).controller(sc);
    SizedBox::exact(200.0, 200.0, if REVERSE.with(|r| r.get()) { l.reverse() } else { l })
}

/// An offset beyond the content (set by a caller, or kept across a remount) still builds
/// and shows the items at the clamped end, never a blank list.
#[test]
fn out_of_range_offset_still_builds_the_visible_items() {
    for reverse in [false, true] {
        REVERSE.with(|r| r.set(reverse));
        let mut h = Harness::new().window(200.0, 200.0);
        h.mount(controlled);
        h.frames(2);
        SC.with(|s| s.get()).unwrap().offset_signal().set(1.0e6);
        h.frames(2);
        assert!(h.find_text("row 99"), "reverse={reverse}: the far end's rows are built");
        assert!(!h.find_text("row 50"), "reverse={reverse}: only the visible window");
    }
}
