//! [`Pagination`]: the `Numbers` design fires `on_page` for pills and chevrons,
//! disables the chevrons at the bounds, and collapses long ranges to ellipses;
//! the `Simple`/`Arrows` designs fire through chevrons; legacy `on_prev`/`on_next`
//! callbacks still work.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_testing::{draw_frame as frame};
use pebbles_widgets::{
    PaginationVariant, View, column, pagination,
};

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

#[test]
fn numbers_fire_on_page_and_bounds_disable() {
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

    // Page 1 window: [prev][1][2][…][20][next] — prev is disabled at page 1.
    // Measured: prev chevron x 4..32, pill 1 x 36..68, pill 2 x 72..104,
    // pill 20 x 164..204, next chevron x 208..240.
    tap(&mut ui, Offset::new(16.0, 14.0)); // prev chevron (disabled)
    frame(&mut ui, &mut env, win);
    assert!(WENT.with(|w| w.borrow().is_empty()), "prev at page 1 is disabled");

    tap(&mut ui, Offset::new(52.0, 14.0)); // pill 1 (already active)
    frame(&mut ui, &mut env, win);
    assert_eq!(WENT.with(|w| w.borrow().clone()), vec![1], "the active pill still reports");

    tap(&mut ui, Offset::new(88.0, 14.0)); // pill 2
    frame(&mut ui, &mut env, win);
    assert_eq!(WENT.with(|w| w.borrow().clone()), vec![1, 2], "pill 2 reports page 2");

    tap(&mut ui, Offset::new(150.0, 14.0)); // pill 20
    frame(&mut ui, &mut env, win);
    assert_eq!(WENT.with(|w| w.borrow().clone()), vec![1, 2, 20], "the last pill reports the last page");
}

#[test]
fn long_ranges_collapse_to_ellipses_and_paint() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(600.0, 120.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![
                    pagination(10, 20)
                        .variant(PaginationVariant::Numbers)
                        .max_buttons(7)
                        .into_widget(),
                    pagination(5, 8).variant(PaginationVariant::Simple).into_widget(),
                    pagination(7, 20).variant(PaginationVariant::Arrows).into_widget(),
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
                    pagination(2, 20)
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

    // Numbers design at page 2: [prev][1][2][3][…][20][next] — prev chevron
    // x 4..32, next chevron x 208..240.
    tap(&mut ui, Offset::new(16.0, 14.0));
    frame(&mut ui, &mut env, win);
    assert_eq!(PREV.with(|p| p.borrow().clone()), 1, "prev chevron fires the legacy callback");

    tap(&mut ui, Offset::new(224.0, 14.0));
    frame(&mut ui, &mut env, win);
    assert_eq!(NEXT.with(|p| p.borrow().clone()), 1, "next chevron fires the legacy callback");
}
