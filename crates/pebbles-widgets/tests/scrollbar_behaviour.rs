//! Desktop scrollbar behaviour (VS Code / Chromium semantics) through the headless
//! harness, which routes presses the way the shell does (a press on a bar grabs it).
//!
//! A 250×200 viewport over 1200 px of content: track = 200 − 2·2 = 196, thumb =
//! 196·200/1200 ≈ 32.7, travel ≈ 163.3, max offset = 1000.

use pebbles_core::IntoWidget;
use pebbles_foundation::{Axis, Offset};
use pebbles_render::{RenderScroll, ScrollUnit};
use pebbles_testing::Harness;
use pebbles_widgets::{SingleChildScrollView, column, container, gap_h, row, scroll_view};

const MAX: f64 = 1000.0;

fn track() -> f64 {
    196.0
}
fn thumb() -> f64 {
    track() * 200.0 / 1200.0
}
fn travel() -> f64 {
    track() - thumb()
}

fn tall() -> Harness {
    let mut h = Harness::new().window(250.0, 200.0);
    let rows: Vec<_> = (0..40).map(|_| gap_h(30.0).into_widget()).collect();
    h.mount_widget(container().width(250.0).height(200.0).child(scroll_view(column(rows))));
    h.frame();
    h
}

fn scroll_of(h: &Harness, axis: Axis) -> (f64, bool, bool) {
    let id = h
        .find_all::<RenderScroll>()
        .into_iter()
        .find(|&id| h.object::<RenderScroll>(id).axis == axis)
        .expect("scroll view");
    let s = h.object::<RenderScroll>(id);
    (s.offset, s.bar_hovered(), s.bar_active())
}

fn offset(h: &Harness) -> f64 {
    scroll_of(h, Axis::Vertical).0
}

#[test]
fn thumb_size_and_position_track_the_viewport() {
    let style = pebbles_render::ScrollbarStyle::default();
    let (ts, tl, hs, hl) = style.geometry(200.0, 1200.0, 500.0);
    assert_eq!((ts, tl), (2.0, 196.0), "the track is inset by the margin");
    assert!((hl - thumb()).abs() < 1e-6, "thumb = track × viewport / content: {hl}");
    assert!((hs - (2.0 + travel() * 0.5)).abs() < 1e-6, "half-way offset → half-way thumb: {hs}");
    let (_, _, end, len) = style.geometry(200.0, 1200.0, MAX);
    assert!((end + len - 198.0).abs() < 1e-6, "at max the thumb ends at the track end");
    assert!((style.offset_for_thumb(200.0, 1200.0, hs) - 500.0).abs() < 1e-6, "inverse mapping");
}

#[test]
fn grabbing_the_thumb_does_not_jump_and_it_follows_proportionally() {
    let mut h = tall();
    // Press near the BOTTOM of the thumb (it starts at y=2): no jump.
    h.button_down(Offset::new(244.0, 2.0 + thumb() - 3.0), pebbles_render::PointerButton::Primary);
    assert_eq!(offset(&h), 0.0, "pressing the thumb must not move it");
    assert!(scroll_of(&h, Axis::Vertical).2, "the thumb is active while dragged");
    // Drag 40 px down: the content moves 40 · max / travel.
    h.pointer_move(Offset::new(244.0, 2.0 + thumb() - 3.0 + 40.0));
    let want = 40.0 * MAX / travel();
    assert!((offset(&h) - want).abs() < 0.5, "proportional drag: {} vs {want}", offset(&h));
    // Keep following while the pointer leaves the bar sideways.
    h.pointer_move(Offset::new(20.0, 2.0 + thumb() - 3.0 + 80.0));
    let want = 80.0 * MAX / travel();
    assert!((offset(&h) - want).abs() < 0.5, "drag outside the bar: {} vs {want}", offset(&h));
    h.button_up(Offset::new(20.0, 120.0), pebbles_render::PointerButton::Primary);
    assert!(!scroll_of(&h, Axis::Vertical).2, "released → not active");
}

#[test]
fn clicking_the_track_centres_the_thumb_on_the_click() {
    let mut h = tall();
    h.click(Offset::new(244.0, 150.0));
    let want = (150.0 - thumb() / 2.0 - 2.0) / travel() * MAX;
    assert!((offset(&h) - want).abs() < 0.5, "jump to the click: {} vs {want}", offset(&h));
}

#[test]
fn hover_state_and_arrow_cursor_over_the_bar() {
    let mut h = tall();
    h.hover(Offset::new(244.0, 50.0));
    assert!(scroll_of(&h, Axis::Vertical).1, "hovered");
    assert!(h.ui.cursor_at(Offset::new(244.0, 50.0)).is_none(), "arrow over the bar");
    h.hover(Offset::new(100.0, 50.0));
    assert!(!scroll_of(&h, Axis::Vertical).1, "hover cleared off the bar");
}

#[test]
fn outer_vertical_bar_is_reachable_over_a_nested_horizontal_scroll() {
    // An editor-shaped tree: the vertical viewport's content is a horizontal scroll view
    // spanning its whole width (long lines). The vertical bar must still be grabbable.
    let mut h = Harness::new().window(250.0, 200.0);
    let wide = container().width(900.0).height(1200.0);
    h.mount_widget(
        container().width(250.0).height(200.0).child(scroll_view(SingleChildScrollView::horizontal(wide))),
    );
    h.frame();
    h.click(Offset::new(244.0, 150.0));
    assert!(offset(&h) > 100.0, "the vertical bar took the press: {}", offset(&h));
}

#[test]
fn shift_wheel_and_sideways_wheel_scroll_horizontally() {
    let mut h = Harness::new().window(250.0, 200.0);
    let wide = row(vec![container().width(900.0).height(150.0).into_widget()]);
    h.mount_widget(container().width(250.0).height(200.0).child(SingleChildScrollView::horizontal(wide)));
    h.frame();
    h.wheel(Offset::new(100.0, 50.0), Offset::new(60.0, 0.0), ScrollUnit::Pixels);
    h.settle();
    assert!((scroll_of(&h, Axis::Horizontal).0 - 60.0).abs() < 0.5, "sideways wheel");
    pebbles_core::keyboard::set_modifiers(true, false, false, false);
    h.wheel(Offset::new(100.0, 50.0), Offset::new(0.0, 40.0), ScrollUnit::Pixels);
    pebbles_core::keyboard::set_modifiers(false, false, false, false);
    h.settle();
    assert!((scroll_of(&h, Axis::Horizontal).0 - 100.0).abs() < 0.5, "Shift+wheel goes sideways");
}
