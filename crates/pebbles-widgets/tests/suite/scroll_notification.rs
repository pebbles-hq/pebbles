//! ScrollNotification: a scroll view's `.on_scroll` callback fires
//! Start → Update* → End around a drag/wheel, reports live metrics, and emits
//! Overscroll when a drag rubber-bands past an edge.

use std::cell::RefCell;

use pebbles_core::{AnyWidget, IntoWidget, Ui, component};
use pebbles_foundation::{MainAxisSize, Offset, Size, palette};
use pebbles_render::{ScrollEvent, ScrollPhysics, TextEnv};
use pebbles_widgets::{ScrollNotification, SingleChildScrollView, View, column, gap_h, scroll_view, text};

thread_local! {
    static LOG: RefCell<Vec<(ScrollEvent, f64)>> = const { RefCell::new(Vec::new()) };
}

fn record(n: ScrollNotification) {
    LOG.with(|l| l.borrow_mut().push((n.event, n.metrics.pixels)));
}

/// A tall (scrollable) drag-scroll viewport that records its notifications.
fn tall(overscroll: bool) -> SingleChildScrollView {
    let mut kids: Vec<AnyWidget> = Vec::new();
    for i in 0..30 {
        kids.push(text(format!("row {i}")).into_widget());
        kids.push(gap_h(40.0).into_widget());
    }
    scroll_view(column(kids).main_axis_size(MainAxisSize::Min))
        .drag_scroll(true)
        .physics(ScrollPhysics { overscroll, ..Default::default() })
        .on_scroll(record)
}

fn tall_plain() -> SingleChildScrollView {
    tall(false)
}
fn tall_over() -> SingleChildScrollView {
    tall(true)
}

fn kinds() -> Vec<ScrollEvent> {
    LOG.with(|l| l.borrow().iter().map(|(e, _)| *e).collect())
}

fn mount(build: fn() -> SingleChildScrollView) -> (Ui, TextEnv) {
    LOG.with(|l| l.borrow_mut().clear());
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.set_test_clock(Some(0.0));
    ui.mount_root(View::new(palette::WHITE, component(build)).into_widget());
    ui.layout(&mut env, Size::new(300.0, 200.0));
    (ui, env)
}

#[test]
fn drag_emits_start_updates_and_a_single_end() {
    let (mut ui, _env) = mount(tall_plain);

    // Drag up 60px → Start then at least one Update.
    assert!(ui.begin_content_drag(Offset::new(150.0, 100.0)));
    assert!(ui.update_content_drag(Offset::new(150.0, 40.0)));
    assert!(ui.end_content_drag(Offset::new(150.0, 40.0)));
    // Settle the spring — End fires when motion stops.
    for _ in 0..300 {
        if !ui.tick_scrolls(1.0 / 60.0) {
            break;
        }
    }

    let ks = kinds();
    assert_eq!(ks.first(), Some(&ScrollEvent::Start), "opens with Start: {ks:?}");
    assert_eq!(ks.last(), Some(&ScrollEvent::End), "closes with End: {ks:?}");
    assert_eq!(ks.iter().filter(|e| matches!(e, ScrollEvent::Start)).count(), 1, "exactly one Start");
    assert_eq!(ks.iter().filter(|e| matches!(e, ScrollEvent::End)).count(), 1, "exactly one End");
    assert!(
        ks.iter().any(|e| matches!(e, ScrollEvent::Update { .. })),
        "at least one Update between them: {ks:?}",
    );

    // The reported offset reached ~60 (the drag distance).
    let peak = LOG.with(|l| l.borrow().iter().map(|(_, px)| *px).fold(0.0_f64, f64::max));
    assert!(peak >= 55.0, "metrics.pixels tracked the drag, peaked at {peak}");
}

#[test]
fn overscroll_past_the_top_emits_an_overscroll_notification() {
    let (mut ui, _env) = mount(tall_over);

    // Pull down 90px from the top: rubber-bands to a negative offset.
    assert!(ui.begin_content_drag(Offset::new(150.0, 40.0)));
    assert!(ui.update_content_drag(Offset::new(150.0, 130.0)));

    let over = LOG.with(|l| {
        l.borrow().iter().find_map(|(e, _)| match e {
            ScrollEvent::Overscroll { overscroll } => Some(*overscroll),
            _ => None,
        })
    });
    assert!(over.is_some_and(|o| o < 0.0), "a top overscroll reports a negative amount: {over:?}");
}

#[test]
fn wheel_scroll_notifies_and_settles() {
    let (mut ui, _env) = mount(tall_plain);

    // A wheel notch over the viewport moves the target; the spring animates there.
    assert!(ui.dispatch_scroll(Offset::new(150.0, 100.0), 80.0));
    for _ in 0..300 {
        if !ui.tick_scrolls(1.0 / 60.0) {
            break;
        }
    }

    let ks = kinds();
    assert_eq!(ks.first(), Some(&ScrollEvent::Start), "wheel opens with Start: {ks:?}");
    assert_eq!(ks.last(), Some(&ScrollEvent::End), "wheel closes with End: {ks:?}");
}
