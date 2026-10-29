//! Public Popover (3.9): clicking the trigger opens arbitrary content in the overlay
//! layer; outside-click dismisses via the scrim. Driven through a real Ui headlessly.

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{OverlayHost, View, container, context_menu, menu_item, overlay, popover, text};

fn root() -> impl IntoWidget {
    // A fixed-size trigger at the top-left so we know where to tap.
    OverlayHost::wrap(
        popover(text("panel body"), container().width(120.0).height(40.0).child(text("Open")))
            .width(200.0)
            .height(120.0),
    )
}

#[test]
fn popover_opens_on_trigger_click_and_dismisses_outside() {
    overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let window = Size::new(500.0, 400.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, window);
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, window);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut env, &mut scene);
    };
    frame(&mut ui);
    // Publish the window size so anchoring/flip math has real bounds.
    overlay::set_window_size(500.0, 400.0);

    assert!(!overlay::is_open(), "closed initially");

    // Tap inside the trigger rect (top-left, ~120x40).
    let hit = Offset::new(40.0, 20.0);
    ui.dispatch_pointer_down(hit);
    ui.dispatch_tap(hit);
    ui.dispatch_pointer_up(hit);
    frame(&mut ui);

    assert!(overlay::is_open(), "clicking the trigger opens the popover");
    // The panel is anchored just below the trigger (top ≈ 0 + 40 + 6 = 46), 200 wide.
    assert!(overlay::over_panel(60.0, 70.0), "a point inside the opened panel");

    // Outside-click dismisses (the overlay scrim handles this; drive it directly).
    overlay::hide_overlay();
    frame(&mut ui);
    assert!(!overlay::is_open(), "dismissed");
}

fn ctx_root() -> impl IntoWidget {
    OverlayHost::wrap(
        context_menu(container().width(160.0).height(90.0).child(text("right-click")))
            .item(menu_item("Copy"))
            .item(menu_item("Paste"))
            .separator()
            .item(menu_item("Delete").destructive()),
    )
}

#[test]
fn context_menu_opens_on_secondary_click_at_the_cursor() {
    overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let window = Size::new(500.0, 400.0);
    ui.mount_root(View::new(palette::WHITE, component(ctx_root)).into_widget());
    ui.layout(&mut env, window);
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, window);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut env, &mut scene);
    };
    frame(&mut ui);
    overlay::set_window_size(500.0, 400.0);

    assert!(!overlay::is_open(), "closed initially");
    // Right-press inside the target.
    assert!(ui.dispatch_secondary_tap_down(Offset::new(40.0, 30.0)), "secondary-tap handled");
    frame(&mut ui);
    assert!(overlay::is_open(), "context menu opened at the cursor");

    overlay::hide_overlay();
    frame(&mut ui);
    assert!(!overlay::is_open(), "dismissed");
}

// ---------------------------------------------------------------------------
// Regression (Gravel IDE): opening/closing an overlay re-renders `OverlayHost`,
// which hands back its stored `child` (the SAME component instance). That used to
// re-run the whole app root every time, so `create_root_signal` state created in the
// root body was recreated (lost) on every tooltip/menu/toast/dialog open or close.
// ---------------------------------------------------------------------------

thread_local! {
    static APP_RUNS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static APP_STATE: std::cell::RefCell<Option<pebbles_core::Signal<i32>>> =
        const { std::cell::RefCell::new(None) };
}

fn stateful_app() -> impl IntoWidget {
    APP_RUNS.with(|c| c.set(c.get() + 1));
    let count = pebbles_core::create_root_signal(0);
    APP_STATE.with(|s| *s.borrow_mut() = Some(count));
    text("app")
}

#[test]
fn overlay_open_close_does_not_rerun_root_or_lose_root_signal_state() {
    overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let window = Size::new(500.0, 400.0);
    ui.mount_root(View::new(palette::WHITE, OverlayHost::wrap(component(stateful_app))).into_widget());
    ui.layout(&mut env, window);
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, window);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut env, &mut scene);
    };
    frame(&mut ui);
    let state = APP_STATE.with(|s| s.borrow().expect("root rendered"));
    state.set(42);
    frame(&mut ui);
    let runs = APP_RUNS.with(std::cell::Cell::get);

    for _ in 0..2 {
        overlay::show_overlay(text("tip").into_widget(), 10.0, 10.0, 80.0, 30.0);
        frame(&mut ui);
        assert!(overlay::is_open(), "overlay opened");
        overlay::hide_overlay();
        frame(&mut ui);
        assert!(!overlay::is_open(), "overlay closed");
    }

    assert_eq!(APP_RUNS.with(std::cell::Cell::get), runs, "overlay open/close must not re-run the app root");
    let now = APP_STATE.with(|s| s.borrow().expect("root rendered"));
    assert_eq!(now.get(), 42, "root-signal state survives overlay open/close");
}
