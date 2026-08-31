//! `date_field().range(true)`: the popover calendar picks a start + end (ordered
//! regardless of tap order), the input shows both dates read-only, and
//! `on_range_changed` reports every completed pick.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{CrossAxisAlignment, Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{Date, OverlayHost, View, column, date_field, overlay};

thread_local! {
    static RANGE: RefCell<Option<(Date, Date)>> = const { RefCell::new(None) };
}

fn root() -> impl IntoWidget {
    OverlayHost::wrap(
        column(vec![
            date_field()
                .range(true)
                .range_value((2026, 1, 1), (2026, 1, 31))
                .width(300.0)
                .on_range_changed(|s, e| RANGE.with(|r| *r.borrow_mut() = Some((s, e))))
                .into_widget(),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start),
    )
}

#[test]
fn range_field_picks_and_reports_ordered_endpoints() {
    overlay::init();
    pebbles_core::focus::init();
    RANGE.with(|r| *r.borrow_mut() = None);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(500.0, 460.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, win);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut scene);
    };
    frame(&mut ui);

    // Open the picker via the trailing calendar button (field is 300 wide; the
    // button sits ~34px from its right edge, centered on y ≈ 19).
    let btn = Offset::new(283.0, 19.0);
    ui.dispatch_pointer_down(btn);
    ui.dispatch_tap(btn);
    ui.dispatch_pointer_up(btn);
    frame(&mut ui);
    assert!(overlay::is_open(), "the calendar popover opens");

    // The popover anchors at ~(8, 53); the grid starts after pad(12) + header(~32)
    // + 10 + weekday row(~14) + 4 → the first cell row centers on y ≈ 145.
    // Jan 2026: Jan 1 = Thursday → day 1 at column 4, day 3 at column 6
    // (x = 8 + 12 + col*40 + 20).
    let day1 = Offset::new(200.0, 144.0);
    let day3 = Offset::new(280.0, 144.0);

    // Pick day 3 first, then day 1 — the endpoints must come back ordered.
    ui.dispatch_pointer_down(day3);
    ui.dispatch_tap(day3);
    ui.dispatch_pointer_up(day3);
    frame(&mut ui);
    ui.dispatch_pointer_down(day1);
    ui.dispatch_tap(day1);
    ui.dispatch_pointer_up(day1);
    frame(&mut ui);

    assert_eq!(
        RANGE.with(|r| r.borrow().clone()),
        Some(((2026, 1, 1), (2026, 1, 3))),
        "on_range_changed reports the ordered range regardless of tap order"
    );
    assert!(!overlay::is_open(), "completing the range closes the popover");
}
