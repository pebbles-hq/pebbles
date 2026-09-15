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
