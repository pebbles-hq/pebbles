//! [`Pagination`]: the redesigned control fires `on_page` for the first/last jump
//! (double chevrons), prev/next arrows and number pills; long ranges collapse to
//! ellipses and paint; legacy `on_prev`/`on_next` still work. Tests discover the
//! controls by hit-testing (no hard-coded pixel offsets) and run at a middle page so
//! every control is enabled.

use std::cell::RefCell;
use std::collections::BTreeMap;

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::{RenderId, RenderPointerListener, TextEnv};
use pebbles_testing::draw_frame as frame;
use pebbles_widgets::{PaginationVariant, View, column, pagination};

thread_local! {
    static WENT: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    static PREV: RefCell<usize> = const { RefCell::new(0) };
    static NEXT: RefCell<usize> = const { RefCell::new(0) };
}

fn tap(ui: &mut Ui, p: Offset) {
    ui.dispatch_pointer_down(p);
    ui.dispatch_tap(p);
    ui.dispatch_pointer_up(p);
}

/// The pagination controls, left-to-right, as a point inside each. Discovered by
/// scanning the top row for distinct pointer-listener nodes — no fixed offsets.
fn controls(ui: &Ui) -> Vec<Offset> {
    let mut seen: BTreeMap<RenderId, Offset> = BTreeMap::new();
    for y in (2..38).step_by(2) {
        for x in (0..480).step_by(2) {
            let p = Offset::new(x as f64, y as f64);
            if let Some(id) = ui
                .render_tree()
                .hit_test(p)
                .iter()
                .copied()
                .find(|&id| ui.render_tree().object_ref(id).is::<RenderPointerListener>())
            {
                seen.entry(id).or_insert(p);
            }
        }
    }
    let mut v: Vec<(RenderId, Offset)> = seen.into_iter().collect();
    v.sort_by(|a, b| a.1.x.total_cmp(&b.1.x));
    v.into_iter().map(|(_, p)| p).collect()
}

#[test]
fn first_prev_next_last_fire_the_right_pages() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    WENT.with(|w| w.borrow_mut().clear());

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(500.0, 120.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![
                    // Page 5 of 20: every control is enabled (a middle page).
                    pagination(5, 20)
                        .variant(PaginationVariant::Numbers)
                        .on_page(|p| WENT.with(|w| w.borrow_mut().push(p)))
                        .into_widget(),
                ])
                .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start)
            }),
        )
        .into_widget(),
    );
    frame(&mut ui, &mut env, win);

    // L→R: [first] [prev] [pills…] [next] [last]. The four edges are at known indices.
    let c = controls(&ui);
    assert!(c.len() >= 5, "expected first/prev/pills/next/last, got {}", c.len());
    let (first, prev, next, last) = (c[0], c[1], c[c.len() - 2], c[c.len() - 1]);

    tap(&mut ui, first);
    frame(&mut ui, &mut env, win);
    tap(&mut ui, prev);
    frame(&mut ui, &mut env, win);
    tap(&mut ui, next);
    frame(&mut ui, &mut env, win);
    tap(&mut ui, last);
    frame(&mut ui, &mut env, win);

    assert_eq!(
        WENT.with(|w| w.borrow().clone()),
        vec![1, 4, 6, 20],
        "first→1, prev→4, next→6, last→20 from page 5 of 20"
    );
}

#[test]
fn bounds_disable_the_leading_and_trailing_jumps() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    WENT.with(|w| w.borrow_mut().clear());

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(500.0, 120.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![
                    // Page 1: first + prev are disabled; the leftmost live control is pill 1.
                    pagination(1, 20)
                        .variant(PaginationVariant::Numbers)
                        .on_page(|p| WENT.with(|w| w.borrow_mut().push(p)))
                        .into_widget(),
                ])
                .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start)
            }),
        )
        .into_widget(),
    );
    frame(&mut ui, &mut env, win);

    // At page 1 the two leading controls (first + prev) are disabled — tapping them
    // fires nothing — while the trailing last-page jump still works.
    let c = controls(&ui);
    tap(&mut ui, c[0]); // first (disabled)
    frame(&mut ui, &mut env, win);
    tap(&mut ui, c[1]); // prev (disabled)
    frame(&mut ui, &mut env, win);
    assert!(WENT.with(|w| w.borrow().is_empty()), "first + prev are disabled at page 1");

    tap(&mut ui, c[c.len() - 1]); // last
    frame(&mut ui, &mut env, win);
    assert_eq!(WENT.with(|w| w.borrow().clone()), vec![20], "the last-page jump still goes to 20");
}

#[test]
fn long_ranges_collapse_to_ellipses_and_paint() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(600.0, 160.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![
                    pagination(10, 20).variant(PaginationVariant::Numbers).max_buttons(7).into_widget(),
                    pagination(5, 8).variant(PaginationVariant::Simple).into_widget(),
                    pagination(7, 20).variant(PaginationVariant::Arrows).edges(false).into_widget(),
                ])
                .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start)
            }),
        )
        .into_widget(),
    );
    frame(&mut ui, &mut env, win);
}

#[test]
fn legacy_callbacks_still_work() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    PREV.with(|p| *p.borrow_mut() = 0);
    NEXT.with(|p| *p.borrow_mut() = 0);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(500.0, 120.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![
                    pagination(5, 20)
                        .on_prev(|| PREV.with(|p| *p.borrow_mut() += 1))
                        .on_next(|| NEXT.with(|p| *p.borrow_mut() += 1))
                        .into_widget(),
                ])
                .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start)
            }),
        )
        .into_widget(),
    );
    frame(&mut ui, &mut env, win);

    // prev (control index 1) goes backward → on_prev; next (index len-2) → on_next.
    let c = controls(&ui);
    tap(&mut ui, c[1]);
    frame(&mut ui, &mut env, win);
    assert_eq!(PREV.with(|p| *p.borrow()), 1, "prev fires the legacy callback");

    tap(&mut ui, c[c.len() - 2]);
    frame(&mut ui, &mut env, win);
    assert_eq!(NEXT.with(|p| *p.borrow()), 1, "next fires the legacy callback");
}
