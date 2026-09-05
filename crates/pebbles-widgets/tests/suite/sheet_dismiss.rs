//! Regression: a tap INSIDE a sheet/dialog must not fall through to the dismiss
//! scrim behind it. The panel consumes taps; only a tap on the scrim (outside the
//! panel) dismisses.

use pebbles_core::{IntoWidget, Ui, animation, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{OverlayHost, Side, View, dialog, overlay, sheet, text};

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
    sheet::sheet(text("sheet content")).side(Side::Bottom).size(300.0).open();
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
fn tapping_inside_a_dialog_keeps_it_open() {
    let (mut ui, mut env, win) = mount();

    dialog::dialog(text("dialog content")).width(300.0).open();
    settle(&mut ui, &mut env, win);
    assert!(dialog::is_open(), "dialog opened");

    // A tap in the centered surface must NOT dismiss.
    ui.dispatch_tap(Offset::new(200.0, 300.0));
    assert!(dialog::is_open(), "a tap inside the dialog keeps it open");

    // A tap in the far corner (outside the ~300px-wide surface) dismisses.
    ui.dispatch_tap(Offset::new(10.0, 10.0));
    assert!(!dialog::is_open(), "a tap outside the dialog surface dismisses it");
}
