//! Regression: a tap INSIDE a sheet/dialog must not fall through to the dismiss
//! scrim behind it. The panel consumes taps; only a tap on the scrim (outside the
//! panel) dismisses. Plus: a popover (dropdown/select) opened FROM a sheet must
//! float ABOVE it and stay clickable — not be occluded by the sheet's panel/scrim.

use std::cell::Cell;
use std::rc::Rc;

use pebbles_core::{IntoWidget, Ui, animation, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{Container, GestureDetector, OverlayHost, Side, View, dialog, overlay, sheet, text};

fn root() -> impl IntoWidget {
    OverlayHost::wrap(text("body"))
}

fn mount() -> (Ui, TextEnv, Size) {
    overlay::init();
    pebbles_core::focus::init();
    sheet::init();
    dialog::init();
    animation::reset();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.make_current();
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    let win = Size::new(400.0, 600.0);
    // The shell publishes this each render; a headless test must set it so the
    // bottom sheet gets a real (window-wide) width.
    overlay::set_window_size(win.width, win.height);
    ui.layout(&mut env, win);
    (ui, env, win)
}

fn settle(ui: &mut Ui, env: &mut TextEnv, win: Size) {
    animation::tick(0.0);
    animation::tick(0.3); // past the 0.22s slide-in → fully in place
    ui.rebuild_if_dirty();
    ui.layout(env, win);
}

#[test]
fn tapping_inside_a_bottom_sheet_keeps_it_open() {
    let (mut ui, mut env, win) = mount();

    // A 300px-tall bottom sheet → the panel occupies y ∈ [300, 600].
    sheet(text("sheet content")).side(Side::Bottom).size(300.0).open();
    settle(&mut ui, &mut env, win);
    assert!(sheet::is_open(), "sheet opened");

    // A tap well inside the panel must NOT dismiss (the old bug fell through here).
    ui.dispatch_tap(Offset::new(200.0, 500.0));
    assert!(sheet::is_open(), "a tap inside the sheet keeps it open");

    // A tap on the scrim (above the panel) dismisses.
    ui.dispatch_tap(Offset::new(200.0, 40.0));
    assert!(!sheet::is_open(), "a tap on the scrim dismisses the sheet");
}

#[test]
fn a_sized_bottom_sheet_is_centered_on_the_cross_axis() {
    let (mut ui, mut env, win) = mount();

    // A 200×100 bottom sheet in a 400×600 window → not full-width, so it centers
    // horizontally: the panel occupies x ∈ [100, 300], y ∈ [500, 600].
    sheet(text("mini")).side(Side::Bottom).width(200.0).height(100.0).open();
    settle(&mut ui, &mut env, win);
    assert!(sheet::is_open(), "sheet opened");

    // A tap inside the centered panel keeps it open.
    ui.dispatch_tap(Offset::new(200.0, 550.0));
    assert!(sheet::is_open(), "tap inside the sized, centered panel keeps it open");

    // A tap at the same height but in the left gap (x = 50 < 100) is on the scrim →
    // dismiss. This only holds if the panel is 200 wide and centered (not full-width).
    ui.dispatch_tap(Offset::new(50.0, 550.0));
    assert!(!sheet::is_open(), "tap beside the centered panel dismisses it");
}

#[test]
fn tapping_inside_a_dialog_keeps_it_open() {
    let (mut ui, mut env, win) = mount();

    dialog(text("dialog content")).width(300.0).open();
    settle(&mut ui, &mut env, win);
    assert!(dialog::is_open(), "dialog opened");

    // A tap in the centered surface must NOT dismiss.
    ui.dispatch_tap(Offset::new(200.0, 300.0));
    assert!(dialog::is_open(), "a tap inside the dialog keeps it open");

    // A tap in the far corner (outside the ~300px-wide surface) dismisses.
    ui.dispatch_tap(Offset::new(10.0, 10.0));
    assert!(!dialog::is_open(), "a tap outside the dialog surface dismisses it");
}

#[test]
fn a_popover_opened_from_a_sheet_floats_above_it_and_is_clickable() {
    // A dropdown/select/combobox opened from inside a sheet lives in the popover
    // layer. It MUST render above the sheet, else its menu is occluded by the
    // sheet's panel + scrim and can neither be seen nor tapped (the reported bug).
    let (mut ui, mut env, win) = mount();

    // A 300px-tall bottom sheet occupies y ∈ [300, 600].
    sheet(text("sheet content")).side(Side::Bottom).size(300.0).open();
    settle(&mut ui, &mut env, win);
    assert!(sheet::is_open(), "sheet opened");

    // A popover menu item sitting OVER the sheet panel (x ∈ [150,250], y ∈ [400,440]).
    let picked = Rc::new(Cell::new(false));
    let flag = picked.clone();
    let item =
        GestureDetector::new(Container::new().width(100.0).height(40.0)).on_tap(move || flag.set(true));
    overlay::show_overlay(item.into_widget(), 150.0, 400.0, 100.0, 40.0);
    settle(&mut ui, &mut env, win);

    // Tapping the menu item (which overlaps the sheet) must reach the popover, proving
    // it is stacked above the sheet — not swallowed by the sheet's panel underneath.
    ui.dispatch_tap(Offset::new(200.0, 420.0));
    assert!(picked.get(), "the popover over the sheet received the tap");
    assert!(sheet::is_open(), "and the sheet stayed open (the popover consumed the tap)");
}

#[test]
fn a_dialog_opened_from_a_sheet_stacks_above_it() {
    // Modal surfaces stack by OPEN ORDER. Open a sheet, then a confirmation dialog
    // from within it (the "view → sheet → delete → confirm" flow): the dialog must
    // render above the sheet, not be buried under the sheet's panel + scrim.
    let (mut ui, mut env, win) = mount();

    // Full-window bottom sheet (covers the whole window while open).
    sheet(text("sheet content")).side(Side::Bottom).size(600.0).open();
    settle(&mut ui, &mut env, win);
    assert!(sheet::is_open(), "sheet opened");

    // Then a dialog from within the sheet (opened LATER → higher sequence → on top).
    let picked = Rc::new(Cell::new(false));
    let flag = picked.clone();
    let body =
        GestureDetector::new(Container::new().width(200.0).height(120.0)).on_tap(move || flag.set(true));
    dialog(body).width(200.0).open();
    settle(&mut ui, &mut env, win);
    assert!(dialog::is_open(), "dialog opened over the sheet");

    // The dialog surface is centered (200×~120 in a 400×600 window). A tap on it must
    // reach the dialog's content — proving the dialog is stacked above the sheet.
    ui.dispatch_tap(Offset::new(200.0, 300.0));
    assert!(picked.get(), "the dialog above the sheet received the tap");
    assert!(dialog::is_open() && sheet::is_open(), "both modals remain (tap hit the dialog body)");
}

#[test]
fn a_sheet_opened_from_a_dialog_stacks_above_it() {
    // The reverse: a sheet opened AFTER a dialog must sit above the dialog. This is
    // what a fixed layer order can never satisfy in both directions — only open order.
    let (mut ui, mut env, win) = mount();

    dialog(text("dialog content")).width(300.0).open();
    settle(&mut ui, &mut env, win);
    assert!(dialog::is_open(), "dialog opened");

    // A full-window bottom sheet opened LATER → higher sequence → paints on top,
    // covering the dialog. A tap anywhere inside the sheet panel must NOT dismiss the
    // dialog behind it (the sheet's panel is now the topmost hit layer there).
    sheet(text("sheet over dialog")).side(Side::Bottom).size(600.0).open();
    settle(&mut ui, &mut env, win);

    ui.dispatch_tap(Offset::new(200.0, 300.0));
    assert!(sheet::is_open(), "the later sheet is on top and stays open");
    assert!(dialog::is_open(), "the dialog underneath was not dismissed by a tap on the sheet");
}
