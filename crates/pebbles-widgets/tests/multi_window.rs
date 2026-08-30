//! Two independent `Ui`s (windows) sharing one reactive runtime — the model that
//! crashed before namespacing components by `(window, element)`. Proves:
//!  * a window's LOCAL signal re-renders only that window,
//!  * a SHARED (app-scope) signal re-renders BOTH — the cross-window IPC channel,
//!  * focus in one window doesn't leak into the other,
//!  * and no panic (the old bug drained/mis-rebuilt the other window's dirty set).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use pebbles_core::focus::{FocusNode, create_focus};
use pebbles_core::{IntoWidget, Signal, Ui, WidgetExt, component, create_signal};
use pebbles_foundation::{Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{View, text};

fn paint(ui: &mut Ui, env: &mut TextEnv, window: Size) {
    ui.rebuild_if_dirty();
    ui.layout(env, window);
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut scene);
}

thread_local! {
    static SHARED: RefCell<Option<Signal<i32>>> = const { RefCell::new(None) };
    static W0_LOCAL: Cell<Option<Signal<i32>>> = const { Cell::new(None) };
    static W1_LOCAL: Cell<Option<Signal<i32>>> = const { Cell::new(None) };
    static W0_RENDERS: Cell<u32> = const { Cell::new(0) };
    static W1_RENDERS: Cell<u32> = const { Cell::new(0) };
    static W0_NODE: Cell<Option<FocusNode>> = const { Cell::new(None) };
    static W1_NODE: Cell<Option<FocusNode>> = const { Cell::new(None) };
    static W0_FOCUSED: Cell<bool> = const { Cell::new(false) };
    static W1_FOCUSED: Cell<bool> = const { Cell::new(false) };
}

fn shared() -> Signal<i32> {
    SHARED.with(|c| c.borrow().unwrap())
}

fn w0_root() -> impl IntoWidget {
    let local = create_signal(0i32);
    W0_LOCAL.with(|c| c.set(Some(local)));
    let node = create_focus();
    node.register(Rc::new(|| {}), None, false);
    W0_NODE.with(|c| c.set(Some(node)));
    W0_FOCUSED.with(|c| c.set(node.is_focused())); // subscribes to the focus signal
    let v = shared().get() + local.get(); // subscribes to both signals
    W0_RENDERS.with(|c| c.set(c.get() + 1));
    text(format!("w0 {v}"))
}

fn w1_root() -> impl IntoWidget {
    let local = create_signal(0i32);
    W1_LOCAL.with(|c| c.set(Some(local)));
    let node = create_focus();
    node.register(Rc::new(|| {}), None, false);
    W1_NODE.with(|c| c.set(Some(node)));
    W1_FOCUSED.with(|c| c.set(node.is_focused()));
    let v = shared().get() + local.get();
    W1_RENDERS.with(|c| c.set(c.get() + 1));
    text(format!("w1 {v}"))
}

#[test]
fn two_windows_share_a_runtime_without_aliasing() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    SHARED.with(|c| *c.borrow_mut() = Some(create_signal(0i32))); // app-scope (global)

    let mut env = TextEnv::new();
    let window = Size::new(300.0, 200.0);
    let mut w0 = Ui::new(); // window 0 (main)
    let mut w1 = Ui::new(); // window 1
    assert_ne!(w0.window_id(), w1.window_id(), "each Ui gets a distinct window id");

    w0.mount_root(View::new(palette::WHITE, component(w0_root)).boxed());
    w1.mount_root(View::new(palette::WHITE, component(w1_root)).boxed());
    w0.layout(&mut env, window);
    w1.layout(&mut env, window);
    assert_eq!(W0_RENDERS.with(Cell::get), 1);
    assert_eq!(W1_RENDERS.with(Cell::get), 1);

    // A LOCAL write re-renders only its own window.
    W0_LOCAL.with(|c| c.get().unwrap()).set(1);
    paint(&mut w0, &mut env, window);
    paint(&mut w1, &mut env, window);
    assert_eq!(W0_RENDERS.with(Cell::get), 2, "window 0 rebuilt");
    assert_eq!(W1_RENDERS.with(Cell::get), 1, "window 1 must NOT rebuild for window 0's signal");

    // A SHARED write re-renders BOTH — this is the IPC channel.
    shared().set(5);
    paint(&mut w0, &mut env, window);
    paint(&mut w1, &mut env, window);
    assert_eq!(W0_RENDERS.with(Cell::get), 3, "window 0 saw the shared write");
    assert_eq!(W1_RENDERS.with(Cell::get), 2, "window 1 saw the shared write");

    // Focus in window 0 must not read as focused in window 1.
    W0_NODE.with(|c| c.get().unwrap()).request_focus();
    paint(&mut w0, &mut env, window);
    paint(&mut w1, &mut env, window);
    assert!(W0_FOCUSED.with(Cell::get), "window 0's node is focused");
    assert!(!W1_FOCUSED.with(Cell::get), "window 1's node is NOT focused (no cross-window leak)");
}
