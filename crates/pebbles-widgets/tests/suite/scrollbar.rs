//! Regression: a scrollbar drag whose scroll view unmounts mid-drag (e.g. the
//! time-picker dropdown closing on a wheel/resize) must not crash on the freed
//! render node. Reproduces "the app crashes when I'm picking time".

use pebbles_core::{IntoWidget, Ui};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{
    OverlayHost, View, column, container, gap_h, hide_overlay, scroll_view, show_overlay, text,
};

#[test]
fn scrollbar_drag_survives_scroll_view_unmount() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let window = Size::new(400.0, 400.0);
    ui.mount_root(View::new(palette::WHITE, OverlayHost::wrap(text("root"))).into_widget());
    ui.layout(&mut env, window);

    // Show an overlay with a tall, scrollable box (like the time-picker dropdown).
    let rows: Vec<_> = (0..40).map(|_| gap_h(30.0).into_widget()).collect();
    let scroller = container().width(250.0).height(200.0).child(scroll_view(column(rows)));
    show_overlay(scroller.into_widget(), 0.0, 0.0, 260.0, 200.0);
    ui.rebuild_if_dirty();
    ui.layout(&mut env, window);

    // Grab the scrollbar at the box's right edge.
    let grabbed = ui.begin_scrollbar_drag(Offset::new(246.0, 100.0));
    assert!(grabbed, "should have grabbed the scrollbar");
    assert!(ui.scrollbar_dragging());

    // Close the overlay — the scroll view (and its scrollbar) unmounts mid-drag.
    hide_overlay();
    ui.rebuild_if_dirty();
    ui.layout(&mut env, window);

    // Continuing to "drag" must not panic on the freed scroll node; it just ends.
    assert!(!ui.update_scrollbar_drag(Offset::new(246.0, 160.0)));
    assert!(!ui.scrollbar_dragging(), "the stale drag should have been dropped");
}
