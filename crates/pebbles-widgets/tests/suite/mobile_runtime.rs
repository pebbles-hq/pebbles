//! Mobile-runtime hooks: `resize_to_avoid_bottom_inset` reacts to reported soft-
//! keyboard insets, `PopScope` intercepts a back dispatch, and `SystemChrome`
//! round-trips the requested overlay style.

use std::cell::{Cell, RefCell};

use pebbles_core::{IntoWidget, Signal, Ui, component, create_root_signal};
use pebbles_foundation::{EdgeInsets, Size, palette};
use pebbles_render::{RenderPadding, TextEnv};
use pebbles_widgets::{
    OverlayHost, SystemUiOverlayStyle, View, back_is_blocked, center, dispatch_back, pop_scope, scaffold,
    set_system_ui_overlay_style, set_view_insets, system_ui_overlay_style, text,
};

fn frame(ui: &mut Ui, env: &mut TextEnv, win: Size) {
    ui.rebuild_if_dirty();
    ui.layout(env, win);
    let mut scene = pebbles_render::Scene::new();
    ui.paint(env, &mut scene);
}

// ---------------------------------------------------------------------------
// resize_to_avoid_bottom_inset
// ---------------------------------------------------------------------------

fn scaffold_root() -> impl IntoWidget {
    OverlayHost::wrap(scaffold(center(text("body"))))
}

/// A distinctive inset so it can't be confused with any other padding node.
const KB: f64 = 137.0;

#[test]
fn scaffold_lifts_above_a_reported_keyboard_inset() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    set_view_insets(EdgeInsets::ZERO); // isolate from prior state

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 600.0);
    ui.mount_root(View::new(palette::WHITE, component(scaffold_root)).into_widget());
    frame(&mut ui, &mut env, win);

    let has_kb_pad = |ui: &Ui| {
        ui.render_tree().find_all::<RenderPadding>().into_iter().any(|id| {
            (ui.render_tree().object_ref(id).downcast_ref::<RenderPadding>().unwrap().insets.bottom - KB)
                .abs()
                < 0.5
        })
    };

    assert!(!has_kb_pad(&ui), "no keyboard padding with zero insets");

    // The shell reports a soft keyboard — the scaffold reactively lifts above it.
    set_view_insets(EdgeInsets { left: 0.0, top: 0.0, right: 0.0, bottom: KB });
    frame(&mut ui, &mut env, win);
    assert!(has_kb_pad(&ui), "the scaffold inset by the reported keyboard height");

    // Keyboard hides → the inset is gone again.
    set_view_insets(EdgeInsets::ZERO);
    frame(&mut ui, &mut env, win);
    assert!(!has_kb_pad(&ui), "the inset clears when the keyboard hides");
}

// ---------------------------------------------------------------------------
// PopScope
// ---------------------------------------------------------------------------

thread_local! {
    static POPS: Cell<u32> = const { Cell::new(0) };
    static SHOW: RefCell<Option<Signal<bool>>> = const { RefCell::new(None) };
}

fn pop_root() -> impl IntoWidget {
    let show = SHOW.with(|c| c.borrow().expect("SHOW set before mount"));
    if show.get() {
        pop_scope(text("screen")).on_pop(|| POPS.with(|p| p.set(p.get() + 1))).into_widget()
    } else {
        text("gone").into_widget()
    }
}

#[test]
fn pop_scope_intercepts_back_while_mounted() {
    POPS.with(|p| p.set(0));
    let show = create_root_signal(true);
    SHOW.with(|c| *c.borrow_mut() = Some(show));

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(300.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(pop_root)).into_widget());
    frame(&mut ui, &mut env, win);

    assert!(back_is_blocked(), "a blocking pop_scope is registered");
    assert!(dispatch_back(), "back is consumed by the pop_scope");
    assert_eq!(POPS.with(Cell::get), 1, "on_pop fired");

    // Unmount the pop_scope → back is no longer intercepted.
    show.set(false);
    ui.rebuild_if_dirty();
    frame(&mut ui, &mut env, win);
    assert!(!back_is_blocked(), "unmounted → nothing blocks back");
    assert!(!dispatch_back(), "back is not consumed once unmounted");
    assert_eq!(POPS.with(Cell::get), 1, "on_pop did not fire again");
}

// ---------------------------------------------------------------------------
// SystemChrome
// ---------------------------------------------------------------------------

#[test]
fn system_chrome_round_trips_the_requested_style() {
    let style = SystemUiOverlayStyle {
        status_bar_color: Some(palette::BLUE),
        status_bar_dark_icons: true,
        nav_bar_color: Some(palette::BLACK),
        nav_bar_dark_icons: false,
    };
    set_system_ui_overlay_style(style);
    assert_eq!(system_ui_overlay_style(), style, "the shell reads back what was requested");
}
