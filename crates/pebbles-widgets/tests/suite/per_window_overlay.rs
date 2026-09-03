//! Per-window overlay + dialog isolation (checklist 1.5): the overlay and modal
//! signals are namespaced by window id, so a popover/dialog opened in one window is
//! invisible to — and independently dismissible from — another. The shell selects the
//! active window with `Ui::make_current` before dispatching input; here we drive that
//! directly with two `Ui`s.

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{OverlayHost, View, dialog, overlay, text};

fn root() -> impl IntoWidget {
    OverlayHost::wrap(text("window body"))
}

fn mount() -> (Ui, TextEnv) {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.make_current(); // key this window's lazily-created overlay/dialog signals
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, Size::new(300.0, 200.0));
    (ui, env)
}

#[test]
fn dialogs_are_isolated_per_window() {
    overlay::init();
    pebbles_core::focus::init();
    dialog::init();

    let (w0, _e0) = mount();
    let (w1, _e1) = mount();
    assert_ne!(w0.window_id(), w1.window_id());

    // Open a dialog in window 1.
    w1.make_current();
    let id = dialog(text("in window 1")).open();
    assert!(dialog::is_open(), "window 1 has an open dialog");

    // Window 0 sees no dialog — full isolation.
    w0.make_current();
    assert!(!dialog::is_open(), "window 0 has no dialog");

    // Closing window 0's (nonexistent) dialog does nothing to window 1's.
    dialog::close_dialog(0);
    w1.make_current();
    assert!(dialog::is_open(), "window 1's dialog is untouched");

    // Now open one in window 0 too — both coexist independently.
    w0.make_current();
    dialog(text("in window 0")).open();
    assert!(dialog::is_open(), "window 0 now has its own dialog");
    w1.make_current();
    assert!(dialog::is_open(), "window 1 still open");

    // Close window 1's by id; window 0's stays.
    dialog::close_dialog(id);
    assert!(!dialog::is_open(), "window 1 closed");
    w0.make_current();
    assert!(dialog::is_open(), "window 0 unaffected by closing window 1's dialog");
}

#[test]
fn overlays_and_window_size_are_isolated_per_window() {
    overlay::init();

    let (w0, _e0) = mount();
    let (w1, _e1) = mount();

    // Distinct sizes per window.
    w0.make_current();
    overlay::set_window_size(300.0, 200.0);
    w1.make_current();
    overlay::set_window_size(800.0, 600.0);
    w0.make_current();
    assert_eq!(overlay::window_size(), (300.0, 200.0));
    w1.make_current();
    assert_eq!(overlay::window_size(), (800.0, 600.0));

    // A popover shown in window 1 is not open in window 0.
    w1.make_current();
    overlay::show_overlay(text("menu").into_widget(), 10.0, 10.0, 50.0, 40.0);
    assert!(overlay::is_open(), "window 1 has an open overlay");
    assert!(overlay::over_panel(20.0, 20.0), "point inside window 1's panel");
    w0.make_current();
    assert!(!overlay::is_open(), "window 0 has no overlay");
    assert!(!overlay::over_panel(20.0, 20.0), "window 0 has no panel to be over");

    // Dismiss in window 1; window 0 was never affected.
    w1.make_current();
    overlay::hide_overlay();
    assert!(!overlay::is_open(), "window 1 dismissed");
}
