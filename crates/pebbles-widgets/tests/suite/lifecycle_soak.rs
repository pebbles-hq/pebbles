//! Slim lifecycle leak-soak — the framework's leak tripwire, kept in-repo after the
//! full route-hopping gallery soak moved to the pebbles-landing repo.
//!
//! Mounts a representative widget set (buttons, text field, checkbox, slider,
//! tooltip, a virtualized list), unmounts it, exercises the overlay + passive
//! layers, and asserts every shared registry returns to a warmed baseline across
//! rounds. A leak that grows round-over-round fails here with its counter's name.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, Signal, Ui, component, create_root_signal};
use pebbles_foundation::{MainAxisSize, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{
    ListView, SizedBox, View, button, checkbox, column, hide_overlay, hide_passive, show_overlay,
    show_passive, slider, text, text_field, tooltip,
};

const WIN: Size = Size::new(600.0, 500.0);

thread_local! {
    static SHOW: RefCell<Option<Signal<bool>>> = const { RefCell::new(None) };
}

/// A representative slice of the catalog — enough registries that a mount/unmount
/// leak (elements, render nodes, signals, cleanups, focus, text-edit, scroll) shows.
fn heavy() -> impl IntoWidget {
    column(vec![
        button("Save").on_pressed(|| {}).into_widget(),
        text_field().placeholder("name").into_widget(),
        checkbox(true).into_widget(),
        slider(50.0).into_widget(),
        tooltip("hint", button("Info")).into_widget(),
        SizedBox::exact(200.0, 150.0, ListView::builder(40, 28.0, |i| text(format!("row {i}"))))
            .into_widget(),
    ])
    .main_axis_size(MainAxisSize::Min)
}

fn root() -> impl IntoWidget {
    let show = SHOW.with(|c| c.borrow().expect("SHOW set before mount"));
    if show.get() { component(heavy).into_widget() } else { text("idle").into_widget() }
}

/// One shell-style frame.
fn frame(ui: &mut Ui, env: &mut TextEnv, now: &mut f64) {
    *now += 0.016;
    pebbles_core::animation::tick(*now);
    ui.tick_scrolls(0.016);
    ui.make_current();
    ui.rebuild_if_dirty();
    ui.layout(env, WIN);
}

/// Pump frames until the animation driver goes idle (bounded).
fn settle(ui: &mut Ui, env: &mut TextEnv, now: &mut f64) {
    for _ in 0..30 {
        frame(ui, env, now);
        if !pebbles_core::animation::active() {
            break;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Census {
    elements: usize,
    render_nodes: usize,
    signals: usize,
    memos: usize,
    subscriptions: usize,
    cleanups: usize,
    pending: usize,
    focus: usize,
    loops: usize,
    timeouts: usize,
    scroll_handlers: usize,
    scroll_metrics: usize,
    text_edit: usize,
    overlays: usize,
    passive: usize,
}

fn census(ui: &Ui) -> Census {
    Census {
        elements: ui.element_count(),
        render_nodes: ui.render_node_count(),
        signals: pebbles_core::census_signals(),
        memos: pebbles_core::census_memos(),
        subscriptions: pebbles_core::census_subscriptions(),
        cleanups: pebbles_core::census_cleanups(),
        pending: pebbles_core::census_pending(),
        focus: pebbles_core::census_registrations(),
        loops: pebbles_core::census_loops(),
        timeouts: pebbles_core::census_timeouts(),
        scroll_handlers: pebbles_core::census_handlers(),
        scroll_metrics: pebbles_render::scroll_metrics::len(),
        text_edit: pebbles_render::text_edit::len(),
        overlays: pebbles_widgets::overlay::census_overlays(),
        passive: pebbles_widgets::overlay::census_passive(),
    }
}

/// Show the heavy set → settle → hide it → settle, plus one overlay + passive cycle.
fn round(ui: &mut Ui, env: &mut TextEnv, now: &mut f64, show: Signal<bool>) {
    show.set(true);
    settle(ui, env, now);

    ui.make_current();
    show_overlay(text("menu").into_widget(), 10.0, 10.0, 200.0, 120.0);
    frame(ui, env, now);
    hide_overlay();
    show_passive(text("tip").into_widget(), 10.0, 10.0);
    frame(ui, env, now);
    hide_passive();

    show.set(false);
    settle(ui, env, now);
}

#[test]
fn lifecycle_soak_returns_to_baseline() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    pebbles_widgets::dialog::init();
    pebbles_core::animation::reset();

    let show = create_root_signal(false);
    SHOW.with(|c| *c.borrow_mut() = Some(show));

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.make_current();
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, WIN);

    let mut now = 0.0_f64;

    // Round 1 warms the true statics (fonts, root signals); baseline is taken after.
    round(&mut ui, &mut env, &mut now, show);
    let baseline = census(&ui);

    for r in 2..=3 {
        round(&mut ui, &mut env, &mut now, show);
        let after = census(&ui);
        assert_eq!(after, baseline, "round {r}: a shared registry grew vs the round-1 baseline");
    }

    assert!(!pebbles_core::animation::active(), "the animation driver never settled after the soak");
}
