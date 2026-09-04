//! [`Accordion`]: self-managed open state with single-open collapse (default) and
//! multiple mode; toggles report `(index, open)`; the chevron-tweening sections
//! lay out and paint.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{CrossAxisAlignment, Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{SizedBox, View, accordion, column};

thread_local! {
    static REPORTS: RefCell<Vec<(usize, bool)>> = const { RefCell::new(Vec::new()) };
}

/// Three sections with fixed 60px content boxes. Section headers are ~41px tall
/// (vertical padding 12 + ~17px text), so with section 0 open (60px content):
/// header 1 ≈ y 102..143. When section 0 collapses, header 1 moves up to
/// ≈ 42..83 and its content fills ≈ 83..143.
fn accordion_view(multiple: bool) -> pebbles_widgets::Accordion {
    accordion()
        .multiple(multiple)
        .item("One", SizedBox::new(Some(200.0), Some(60.0), None))
        .item("Two", SizedBox::new(Some(200.0), Some(60.0), None))
        .item("Three", SizedBox::new(Some(200.0), Some(60.0), None))
        .default_open(0)
        .on_toggle(|i, open| REPORTS.with(|r| r.borrow_mut().push((i, open))))
}

fn setup(view: fn() -> pebbles_widgets::Accordion) -> (Ui, TextEnv, Size) {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    REPORTS.with(|r| r.borrow_mut().clear());
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 400.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            column(vec![component(view).into_widget()]).cross_axis_alignment(CrossAxisAlignment::Stretch),
        )
        .into_widget(),
    );
    ui.layout(&mut env, win);
    (ui, env, win)
}

fn tap(ui: &mut Ui, x: f64, y: f64) -> bool {
    let p = Offset::new(x, y);
    ui.dispatch_pointer_down(p);
    let handled = ui.dispatch_tap(p);
    ui.dispatch_pointer_up(p);
    handled
}

#[test]
fn single_open_collapses_siblings() {
    let (mut ui, mut env, win) = setup(|| accordion_view(false));
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, win);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut env, &mut scene);
    };
    frame(&mut ui);
    let reports = || REPORTS.with(|r| r.borrow().clone());

    // Section 0 is open by default; tapping its header closes it.
    tap(&mut ui, 200.0, 15.0);
    frame(&mut ui);
    assert_eq!(reports(), vec![(0, false)], "tapping the open header closes it");

    // Re-open 0, then open 1: single mode auto-collapses 0, so header 1 moves UP
    // to y ≈ 42..83 and its content fills y ≈ 83..143 — the point (200, 120) is
    // now inside 1's CONTENT (nothing tappable), not its header.
    tap(&mut ui, 200.0, 15.0);
    frame(&mut ui);
    tap(&mut ui, 200.0, 120.0);
    frame(&mut ui);
    assert_eq!(reports().last(), Some(&(1, true)), "opening section 1 reports (1, true)");
    assert!(
        !tap(&mut ui, 200.0, 120.0),
        "after collapsing, the old header-1 position is content — no tap target"
    );

    // The relocated header 1 now sits at y ≈ 42..83.
    assert!(tap(&mut ui, 200.0, 60.0), "the moved header 1 is tappable at its new position");
    frame(&mut ui);
    assert_eq!(reports().last(), Some(&(1, false)), "and toggles back closed");
}

#[test]
fn multiple_mode_keeps_sections_open() {
    let (mut ui, mut env, win) = setup(|| accordion_view(true));
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, win);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut env, &mut scene);
    };
    frame(&mut ui);
    let reports = || REPORTS.with(|r| r.borrow().clone());

    // Open 1 while 0 is open (multiple: both stay open).
    tap(&mut ui, 200.0, 120.0);
    frame(&mut ui);
    assert_eq!(reports(), vec![(1, true)]);

    // Close 0 — section 1 must STAY open: with 0 collapsed, header 1 is at
    // y ≈ 42..83 and reports (1, false) when tapped.
    tap(&mut ui, 200.0, 15.0);
    frame(&mut ui);
    tap(&mut ui, 200.0, 60.0);
    frame(&mut ui);
    assert_eq!(
        reports(),
        vec![(1, true), (0, false), (1, false)],
        "in multiple mode, toggling one section leaves the other open"
    );
}
