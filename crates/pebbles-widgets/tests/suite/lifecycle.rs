//! Frame-loop hygiene: an idle app must not keep the animation driver active, and a
//! closed window must fully leave the shared runtime.
//!
//! * The TextField caret-blink loop ticks ONLY while the field is focused — an
//!   unconditional loop kept `animation::tick` (and therefore the shell's redraw
//!   chain) running at full rate whenever any text field was merely on screen.
//! * `Ui::dispose` + `window::drop_window_state` tear a closed window's components,
//!   loops, and per-window service signals out of the shared runtime — without them
//!   every window open/close leaked the tree, and any spinner in it spun forever.

use pebbles_core::{IntoWidget, Ui, animation, component};
use pebbles_foundation::{Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{OverlayHost, View, dialog, spinner, text, text_field, toast, window};

/// One shell-style frame: reconcile, lay out, then advance the animation driver.
/// Returns whether the driver still has active work (the shell's keep-drawing bit).
fn frame(ui: &mut Ui, env: &mut TextEnv, now: f64) -> bool {
    ui.make_current();
    ui.rebuild_if_dirty();
    ui.layout(env, Size::new(400.0, 300.0));
    animation::tick(now)
}

#[test]
fn caret_blink_ticks_only_while_focused() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    animation::reset();

    // An idle (unfocused) field must not start any loop — a form on screen sits at
    // zero frames.
    let mut idle = Ui::new();
    let mut env_i = TextEnv::new();
    idle.make_current();
    idle.mount_root(
        View::new(palette::WHITE, component(|| OverlayHost::wrap(text_field().width(200.0))))
            .into_widget(),
    );
    idle.layout(&mut env_i, Size::new(400.0, 300.0));
    assert!(!frame(&mut idle, &mut env_i, 0.1), "unfocused field keeps the driver idle");

    // A focused field blinks (loop active)…
    let mut focused = Ui::new();
    let mut env_f = TextEnv::new();
    focused.make_current();
    focused.mount_root(
        View::new(
            palette::WHITE,
            component(|| OverlayHost::wrap(text_field().autofocus().width(200.0))),
        )
        .into_widget(),
    );
    focused.layout(&mut env_f, Size::new(400.0, 300.0));
    frame(&mut focused, &mut env_f, 0.2); // autofocus lands → re-render with focus
    assert!(frame(&mut focused, &mut env_f, 0.3), "focused field runs the blink loop");

    // …and blurring stops it again. Blur also starts the focus-ring fade-out (a ~0.14s
    // `animated` tween), so the driver legitimately stays busy for a few frames — the
    // point of the fix is that it *settles* to idle instead of blinking forever. Pump
    // frames on an advancing clock and require the driver to go quiet within the fade
    // window (the pre-fix caret loop never would).
    pebbles_core::focus::set_focus(None);
    let mut now = 0.4;
    let mut settled = false;
    for _ in 0..20 {
        now += 0.05;
        if !frame(&mut focused, &mut env_f, now) {
            settled = true;
            break;
        }
    }
    assert!(settled, "after blur the driver settles to idle — the blink loop is gone");
}

#[test]
fn disposed_window_leaves_the_runtime() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    animation::reset();

    // Stand-in for the main window.
    let mut main = Ui::new();
    let mut env_m = TextEnv::new();
    main.make_current();
    main.mount_root(
        View::new(palette::WHITE, component(|| OverlayHost::wrap(text("main")))).into_widget(),
    );
    main.layout(&mut env_m, Size::new(300.0, 200.0));

    // A secondary window whose content animates forever.
    let mut win = Ui::new();
    let mut env_w = TextEnv::new();
    win.make_current();
    win.mount_root(
        View::new(palette::WHITE, component(|| OverlayHost::wrap(spinner(24.0)))).into_widget(),
    );
    win.layout(&mut env_w, Size::new(300.0, 200.0));
    assert!(frame(&mut win, &mut env_w, 0.1), "spinner loop is running");

    // Close it the way the shell does: dispose the tree, drop per-window state.
    win.dispose();
    window::drop_window_state(win.window_id());

    // The loop died with its component — the driver goes idle…
    assert!(!animation::tick(0.2), "closed window's loop is gone");
    // …and the main window still frames cleanly.
    assert!(!frame(&mut main, &mut env_m, 0.3));
}

#[test]
fn drop_window_state_clears_per_window_services() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    dialog::init();
    animation::reset();

    let mut win = Ui::new();
    let mut env = TextEnv::new();
    win.make_current();
    win.mount_root(
        View::new(palette::WHITE, component(|| OverlayHost::wrap(text("secondary"))))
            .into_widget(),
    );
    win.layout(&mut env, Size::new(300.0, 200.0));

    win.make_current();
    toast::toast("saved").show();
    assert!(toast::any_open(), "toast lives in this window's stack");
    let _id = dialog::dialog(text("modal")).open();
    assert!(dialog::is_open(), "dialog open in this window");

    win.dispose();
    window::drop_window_state(win.window_id());

    // Same window id queried again → the maps must have forgotten it entirely.
    win.make_current();
    assert!(!toast::any_open(), "toast state dropped with the window");
    assert!(!dialog::is_open(), "dialog state dropped with the window");
}
