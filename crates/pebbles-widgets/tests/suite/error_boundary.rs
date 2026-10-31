//! `error_boundary` catches panics — both while building the subtree and inside a
//! descendant component's render during reconcile — and shows its fallback instead of
//! crashing the app.

use std::cell::Cell;

use pebbles_core::widget::AnyWidget;
use pebbles_core::{IntoWidget, component};
use pebbles_testing::Harness;
use pebbles_widgets::{error_boundary, text};

thread_local! {
    static FALLBACK_SHOWN: Cell<bool> = const { Cell::new(false) };
}

fn mark_fallback() -> AnyWidget {
    FALLBACK_SHOWN.with(|f| f.set(true));
    text("error").into_widget()
}

// --- build-time: content construction panics -------------------------------

fn boom_at_build() -> AnyWidget {
    panic!("boom while building content")
}

fn app_build_panic() -> impl IntoWidget {
    error_boundary(boom_at_build, mark_fallback)
}

#[test]
fn catches_a_build_time_panic() {
    FALLBACK_SHOWN.with(|f| f.set(false));
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut h = Harness::new().window(300.0, 100.0);
    h.mount(app_build_panic);
    h.draw();
    std::panic::set_hook(prev);
    assert!(FALLBACK_SHOWN.with(|f| f.get()), "a panic building the content shows the fallback");
}

// --- render-time: a descendant COMPONENT panics during reconcile -----------

fn boom_at_render() -> AnyWidget {
    panic!("boom in a descendant render")
}

fn app_render_panic() -> impl IntoWidget {
    // The content builds fine (it just wraps a component); the panic happens when the
    // reconciler renders that component — the render-time path.
    error_boundary(|| component(boom_at_render), mark_fallback)
}

#[test]
fn catches_a_descendant_render_panic() {
    FALLBACK_SHOWN.with(|f| f.set(false));
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut h = Harness::new().window(300.0, 100.0);
    h.mount(app_render_panic);
    h.draw(); // frame 1: the descendant panics → boundary is tripped
    h.draw(); // frame 2: the boundary re-renders to its fallback
    std::panic::set_hook(prev);
    assert!(
        FALLBACK_SHOWN.with(|f| f.get()),
        "a panic in a descendant render is caught and shows the fallback"
    );
}

// --- no boundary, app-installed handler: the app stays alive -------------------

thread_local! {
    static BROKEN: Cell<bool> = const { Cell::new(true) };
    static HANDLED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn sometimes_boom() -> AnyWidget {
    let tick = pebbles_core::create_signal(0u32);
    let _ = tick.get();
    TICK.with(|t| t.set(Some(tick)));
    if BROKEN.with(Cell::get) {
        panic!("start byte index 62 is not a char boundary");
    }
    text("recovered").into_widget()
}

thread_local! {
    static TICK: Cell<Option<pebbles_core::Signal<u32>>> = const { Cell::new(None) };
}

fn app_unguarded() -> impl IntoWidget {
    pebbles_widgets::column(vec![text("alive").into_widget(), component(sometimes_boom).into_widget()])
}

#[test]
fn render_panic_handler_keeps_the_app_alive_without_a_boundary() {
    BROKEN.with(|b| b.set(true));
    pebbles_core::set_render_panic_handler(|msg| HANDLED.with(|h| h.borrow_mut().push(msg.to_string())));
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut h = Harness::new().window(300.0, 100.0);
    h.mount(app_unguarded);
    h.draw();
    std::panic::set_hook(prev);
    assert!(HANDLED.with(|h| h.borrow().iter().any(|m| m.contains("char boundary"))));
    assert!(!h.find_all_text("alive").is_empty(), "the rest of the UI still renders");
    assert!(h.find_all_text("recovered").is_empty());
    // The failed component re-renders normally once its data is fixed.
    BROKEN.with(|b| b.set(false));
    if let Some(t) = TICK.with(Cell::get) {
        t.set(1);
    }
    h.frames(2);
    pebbles_core::clear_render_panic_handler();
    assert!(!h.find_all_text("recovered").is_empty(), "re-renders after the fault clears");
}

// --- no boundary, no handler: unwinding tears the half-built tree down safely -----------

thread_local! {
    static FAIL_RERENDER: Cell<Option<pebbles_core::Signal<u32>>> = const { Cell::new(None) };
}

fn fails_on_rerender() -> AnyWidget {
    let n = pebbles_core::create_signal(0u32);
    FAIL_RERENDER.with(|t| t.set(Some(n)));
    pebbles_core::create_cleanup(|| panic!("a cleanup that panics"));
    if n.get() > 0 {
        panic!("boom while re-rendering a mounted subtree");
    }
    pebbles_widgets::column(vec![text("child").into_widget(), text("child 2").into_widget()]).into_widget()
}

fn app_rerender_panic() -> impl IntoWidget {
    pebbles_widgets::column(vec![component(fails_on_rerender).into_widget()])
}

/// Owner crash 2026-11-01: an uncaught render panic unwound through `Harness`/window
/// `Drop` → `dispose` → `unmount`, which indexed an element the failed render had already
/// removed — a second panic during cleanup, aborting the process. Now the panic surfaces
/// once (catchable), and teardown of the failing subtree succeeds.
#[test]
fn an_uncaught_render_panic_unwinds_without_a_double_panic() {
    pebbles_core::clear_render_panic_handler();
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = std::panic::catch_unwind(|| {
        let mut h = Harness::new().window(300.0, 100.0);
        h.mount(app_rerender_panic);
        h.draw();
        if let Some(n) = FAIL_RERENDER.with(Cell::get) {
            n.set(1);
        }
        h.frames(2); // panics; `h` is dropped while unwinding
    });
    std::panic::set_hook(prev);
    assert!(result.is_err(), "the render panic surfaces once, as an ordinary panic");
}
