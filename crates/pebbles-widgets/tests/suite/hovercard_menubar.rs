//! HoverCard (delayed passive card that persists while hovered) and Menubar
//! (click-open strip with hover-switch), driven headlessly through the Ui + animation
//! driver + overlay layer.

use pebbles_core::{IntoWidget, Ui, animation, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{container, hover_card, menu_item, menubar, overlay, OverlayHost, text, tooltip, View};

fn frame(ui: &mut Ui, env: &mut TextEnv, win: Size) {
    ui.rebuild_if_dirty();
    ui.layout(env, win);
    let mut scene = pebbles_render::Scene::new();
    ui.paint(env, &mut scene);
}

fn hc_root() -> impl IntoWidget {
    OverlayHost::wrap(
        pebbles_widgets::column(pebbles_core::children![
            hover_card(text("rich card body"), container().width(90.0).height(28.0).child(text("@user")))
                .delay(0.2),
            container().width(300.0).height(240.0), // empty area to hover off onto
        ])
        .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start)
        .main_axis_size(pebbles_foundation::MainAxisSize::Min),
    )
}

#[test]
fn hover_card_shows_after_delay_and_hides_after_exit() {
    overlay::init();
    pebbles_core::focus::init();
    animation::reset();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 320.0);
    ui.mount_root(View::new(palette::WHITE, component(hc_root)).into_widget());
    ui.layout(&mut env, win);
    overlay::set_window_size(400.0, 320.0);
    frame(&mut ui, &mut env, win);

    assert!(!overlay::passive_is_open(), "no card before hover");
    ui.dispatch_hover(Offset::new(40.0, 14.0)); // over the trigger
    frame(&mut ui, &mut env, win);
    animation::tick(0.01);
    assert!(!overlay::passive_is_open(), "not before the delay");
    animation::tick(0.30);
    frame(&mut ui, &mut env, win);
    assert!(overlay::passive_is_open(), "card shows after the delay");

    // Move off the trigger onto the empty area → close after the close-delay.
    ui.dispatch_hover(Offset::new(40.0, 150.0));
    frame(&mut ui, &mut env, win);
    animation::tick(0.01); // arm the close timer
    animation::tick(0.40); // fire it
    frame(&mut ui, &mut env, win);
    assert!(!overlay::passive_is_open(), "card hides after leaving trigger + card");
}

fn tooltip_root() -> impl IntoWidget {
    OverlayHost::wrap(
        pebbles_widgets::column(pebbles_core::children![
            tooltip("Saved to disk", container().width(90.0).height(28.0)).delay(0.2),
            container().width(300.0).height(240.0), // empty area to hover off onto
        ])
        .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start)
        .main_axis_size(pebbles_foundation::MainAxisSize::Min),
    )
}

#[test]
fn tooltip_shows_after_delay_and_hides_immediately_on_exit() {
    overlay::init();
    pebbles_core::focus::init();
    animation::reset();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 320.0);
    ui.mount_root(View::new(palette::WHITE, component(tooltip_root)).into_widget());
    ui.layout(&mut env, win);
    overlay::set_window_size(400.0, 320.0);
    frame(&mut ui, &mut env, win);

    assert!(!overlay::passive_is_open(), "no tooltip before hover");
    ui.dispatch_hover(Offset::new(40.0, 14.0)); // over the trigger
    frame(&mut ui, &mut env, win);
    animation::tick(0.01);
    assert!(!overlay::passive_is_open(), "not before the delay");
    animation::tick(0.30);
    frame(&mut ui, &mut env, win);
    assert!(overlay::passive_is_open(), "tooltip shows after the delay");

    // Tooltips dismiss immediately on hover-exit (no close grace like HoverCard).
    ui.dispatch_hover(Offset::new(40.0, 150.0));
    frame(&mut ui, &mut env, win);
    assert!(!overlay::passive_is_open(), "tooltip hides on leaving the trigger");
}

fn mb_root() -> impl IntoWidget {
    // A menubar sits at the top of an app, so anchor it top-left with a column
    // (the bare row would otherwise be cross-axis-centered by the full-height host).
    OverlayHost::wrap(
        pebbles_widgets::column(pebbles_core::children![
            menubar()
                .menu("File", [menu_item("New"), menu_item("Open")])
                .menu("Edit", [menu_item("Undo"), menu_item("Redo")]),
            container().width(400.0).height(300.0), // page body
        ])
        .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start)
        .main_axis_size(pebbles_foundation::MainAxisSize::Min),
    )
}

#[test]
fn menubar_opens_on_click_and_switches_on_hover() {
    overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(500.0, 400.0);
    ui.mount_root(View::new(palette::WHITE, component(mb_root)).into_widget());
    ui.layout(&mut env, win);
    overlay::set_window_size(500.0, 400.0);
    frame(&mut ui, &mut env, win);

    assert!(!overlay::is_open(), "closed initially");
    // Click "File" (first trigger, top-left).
    let file = Offset::new(20.0, 16.0);
    ui.dispatch_pointer_down(file);
    ui.dispatch_tap(file);
    ui.dispatch_pointer_up(file);
    frame(&mut ui, &mut env, win);
    assert!(overlay::is_open(), "clicking a trigger opens its menu");

    // Hover the "Edit" trigger (further right) → menu stays open (switched).
    ui.dispatch_hover(Offset::new(70.0, 16.0));
    frame(&mut ui, &mut env, win);
    assert!(overlay::is_open(), "hovering a sibling keeps a menu open (switched)");

    overlay::hide_overlay();
    frame(&mut ui, &mut env, win);
    assert!(!overlay::is_open(), "dismissed");
}
