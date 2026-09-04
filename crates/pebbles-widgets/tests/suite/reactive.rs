//! Headless proof of the SolidJS-style reactive loop: a function component reads a
//! signal, a plain-closure tap handler writes it, and the framework re-renders the
//! component and reconciles — all without a window or GPU.

use std::cell::Cell;

use pebbles_core::{Element, IntoWidget, Ui, action, component, create_effect, create_signal};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::{RenderConstrainedBox, TextEnv};
use pebbles_widgets::{SizedBox, View, center, gesture_detector};

/// A component whose visible width encodes how many times it was tapped.
fn probe() -> Element {
    let taps = create_signal(0);
    let bump = action(move || taps.update(|t| *t += 1));
    gesture_detector(center(SizedBox::new(Some(10.0 + taps.get() as f64 * 10.0), Some(10.0), None)))
        .on_tap(bump)
        .into_widget()
}

fn probe_width(ui: &Ui) -> f64 {
    let tree = ui.render_tree();
    let id = tree.find::<RenderConstrainedBox>().expect("probe SizedBox present");
    tree.size_of(id).width
}

#[test]
fn signal_write_re_renders_component() {
    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    let window = Size::new(200.0, 200.0);

    ui.mount_root(View::new(palette::WHITE, component(probe)).into_widget());
    ui.layout(&mut text, window);
    assert_eq!(probe_width(&ui), 10.0, "initial: taps == 0");

    // A tap fires the plain closure → signal.update → schedules the component.
    assert!(ui.dispatch_tap(Offset::new(100.0, 100.0)), "tap handled");
    assert!(ui.rebuild_if_dirty(), "signal write marks the component dirty");
    ui.layout(&mut text, window);
    assert_eq!(probe_width(&ui), 20.0, "after one tap: taps == 1");

    for _ in 0..3 {
        assert!(ui.dispatch_tap(Offset::new(100.0, 100.0)));
        ui.rebuild_if_dirty();
    }
    ui.layout(&mut text, window);
    assert_eq!(probe_width(&ui), 50.0, "after four taps: taps == 4");
}

thread_local! {
    static EFFECT_RUNS: Cell<u32> = const { Cell::new(0) };
}

/// A component that mimics the `create_resource` / `ImageView` shape: an effect that
/// writes a signal the component itself reads. If effects were recreated on every
/// render (the pre-fix bug), each render would spawn a fresh effect that writes the
/// signal, re-rendering the component and spawning yet another effect — an unbounded
/// loop that pinned the frame loop and leaked an effect (and, in `ImageView`, a network
/// thread) every frame. With position-stable effects the effect is created once and
/// never re-created by a re-render.
fn effect_probe() -> Element {
    let state = create_signal(0u32);
    create_effect(move || {
        EFFECT_RUNS.with(|c| c.set(c.get() + 1));
        // Write the signal the component reads (below) — the loop trigger.
        state.set(1);
    });
    let _ = state.get(); // subscribe: a write to `state` re-renders this component
    center(SizedBox::new(Some(10.0), Some(10.0), None)).into_widget()
}

#[test]
fn component_effect_is_created_once_and_does_not_spin() {
    EFFECT_RUNS.with(|c| c.set(0));
    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    let window = Size::new(100.0, 100.0);

    ui.mount_root(View::new(palette::WHITE, component(effect_probe)).into_widget());
    ui.layout(&mut text, window);

    // Drive reconciliation to convergence. The effect's write dirties the component
    // once; a correct runtime settles immediately after. A capped loop turns the
    // pre-fix infinite loop into a test FAILURE (hitting the cap) instead of a hang.
    let mut rebuilds = 0;
    for _ in 0..1000 {
        if !ui.rebuild_if_dirty() {
            break;
        }
        rebuilds += 1;
    }
    assert!(rebuilds < 1000, "reconciliation never converged — effect is spinning");

    // The effect ran exactly once despite the re-render its own write triggered.
    assert_eq!(EFFECT_RUNS.with(Cell::get), 1, "a component effect must be created once, not per render");
}

thread_local! {
    static SIG_RUNS: Cell<u32> = const { Cell::new(0) };
    static DEP: std::cell::RefCell<Option<pebbles_core::Signal<u32>>> =
        const { std::cell::RefCell::new(None) };
}

fn dep() -> pebbles_core::Signal<u32> {
    DEP.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            *c = Some(create_signal(0));
        }
        c.unwrap()
    })
}

/// An effect whose ONLY input is a signal (the ImageView/create_resource pattern,
/// post-fix). It must run once on mount, NOT re-run on unrelated re-renders, re-run
/// when its signal changes, and die with its component.
fn sig_effect_probe() -> Element {
    create_effect(move || {
        SIG_RUNS.with(|r| r.set(r.get() + 1));
        let _ = dep().get(); // subscribe to the input
    });
    center(SizedBox::new(Some(10.0), Some(10.0), None)).into_widget()
}

#[test]
fn signal_driven_effect_reruns_only_on_its_signal() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    SIG_RUNS.with(|r| r.set(0));
    dep().set(0);

    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    let window = Size::new(100.0, 100.0);
    ui.mount_root(View::new(palette::WHITE, component(sig_effect_probe)).into_widget());
    ui.layout(&mut text, window);
    ui.rebuild_if_dirty();
    assert_eq!(SIG_RUNS.with(Cell::get), 1, "runs once on mount");

    // An unrelated re-render must not re-run it.
    ui.rebuild_if_dirty();
    ui.rebuild_if_dirty();
    assert_eq!(SIG_RUNS.with(Cell::get), 1, "no re-run on re-render");

    // Its input signal changing DOES re-run it.
    dep().set(1);
    ui.rebuild_if_dirty();
    assert_eq!(SIG_RUNS.with(Cell::get), 2, "re-runs when its signal changes");

    // Disposing the component stops it for good.
    ui.dispose();
    dep().set(2);
    ui.rebuild_if_dirty();
    assert_eq!(SIG_RUNS.with(Cell::get), 2, "disposed with its component");
}
