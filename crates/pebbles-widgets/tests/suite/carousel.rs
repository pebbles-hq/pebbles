//! A6: the Carousel — snap-paged slides, dots, arrows, autoplay (paused on
//! hover) and the programmatic controller.

use std::cell::{Cell, RefCell};

use pebbles_core::{IntoWidget, Ui, animation, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{
    CarouselController, StyleExt, View, carousel, center, style, text, use_carousel_controller,
};

thread_local! {
    static CTL: RefCell<Option<CarouselController>> = const { RefCell::new(None) };
    static PAGE_EVENTS: Cell<u32> = const { Cell::new(0) };
}

fn slide(label: &'static str, color: pebbles_foundation::Color) -> impl IntoWidget {
    center(text(label).size(18.0).semibold().color(palette::WHITE)).styled(style().background(color))
}

fn carousel_root() -> impl IntoWidget {
    let controller = use_carousel_controller();
    CTL.with(|c| *c.borrow_mut() = Some(controller));
    carousel(pebbles_core::children![
        slide("one", palette::BLUE),
        slide("two", palette::GREEN),
        slide("three", palette::AMBER),
    ])
    .height(140.0)
    .controller(controller)
    .on_page_changed(|_| PAGE_EVENTS.with(|p| p.set(p.get() + 1)))
}

fn frame(ui: &mut Ui, env: &mut TextEnv, win: Size) {
    ui.rebuild_if_dirty();
    ui.layout(env, win);
}

#[test]
fn carousel_arrows_page_and_report() {
    CTL.with(|c| *c.borrow_mut() = None);
    PAGE_EVENTS.with(|p| p.set(0));
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(320.0, 160.0);
    ui.mount_root(View::new(palette::WHITE, component(carousel_root)).into_widget());
    frame(&mut ui, &mut env, win);
    frame(&mut ui, &mut env, win); // the width probe lands → second render
    let ctl = CTL.with(|c| c.borrow().expect("controller captured"));
    assert_eq!(ctl.page(), 0, "starts on the first page");

    // The next arrow sits at the right edge; tapping it pages forward.
    let next_point = Offset::new(320.0 - 22.0, 70.0);
    assert!(ui.dispatch_tap(next_point), "next arrow tappable");
    frame(&mut ui, &mut env, win);
    frame(&mut ui, &mut env, win);
    assert_eq!(ctl.page(), 1, "next arrow advanced a page");
    assert!(PAGE_EVENTS.with(Cell::get) >= 1, "on_page_changed fired");

    // Programmatic jump via the controller.
    ctl.jump(2);
    frame(&mut ui, &mut env, win);
    frame(&mut ui, &mut env, win);
    assert_eq!(ctl.page(), 2, "controller jump");

    // prev() clamps at page 0.
    ctl.jump(0);
    frame(&mut ui, &mut env, win);
    ctl.prev();
    frame(&mut ui, &mut env, win);
    assert_eq!(ctl.page(), 0, "prev clamps at the first page");
}

#[test]
fn carousel_autoplay_advances_and_pauses_on_hover() {
    CTL.with(|c| *c.borrow_mut() = None);
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(320.0, 160.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                let controller = use_carousel_controller();
                CTL.with(|c| *c.borrow_mut() = Some(controller));
                carousel(pebbles_core::children![
                    slide("one", palette::BLUE),
                    slide("two", palette::GREEN),
                    slide("three", palette::AMBER),
                ])
                .height(140.0)
                .autoplay(2.0)
                .controller(controller)
            }),
        )
        .into_widget(),
    );
    frame(&mut ui, &mut env, win);
    frame(&mut ui, &mut env, win);
    let ctl = CTL.with(|c| c.borrow().expect("controller captured"));
    assert_eq!(ctl.page(), 0);

    // Autoplay ticks: after the 2s period wraps, the page advances.
    for i in 0..(60 * 5) {
        animation::tick(i as f64 / 60.0);
        frame(&mut ui, &mut env, win);
    }
    assert!(ctl.page() > 0, "autoplay advanced: {}", ctl.page());

    // Unmount: the loop dies with the component (lifecycle §0).
    ui.dispose();
    assert!(!animation::tick(999.0), "driver idle after unmount");
}
