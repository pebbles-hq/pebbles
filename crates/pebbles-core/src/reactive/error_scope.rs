//! Render-time error boundary plumbing — the core half of `<ErrorBoundary>`.
//!
//! The `error_boundary` **widget** (in `pebbles-widgets`) provides an
//! [`ErrorBoundaryHandle`] into the render-time context. When the reconciler catches a
//! panic from a descendant component's render, it walks the context stack for the
//! nearest handle and **trips** it — flipping a signal the boundary reads, so the
//! boundary re-renders to its fallback instead of the app crashing.

use super::{Signal, consume_context};

/// A handle a `error_boundary` widget provides into context so the reconciler can
/// route a descendant render panic to it. Wraps the boundary's "errored" signal.
#[derive(Clone, Copy)]
pub struct ErrorBoundaryHandle {
    errored: Signal<bool>,
}

impl ErrorBoundaryHandle {
    /// Wrap a boundary's `errored` signal (create it as a hook in the boundary
    /// component so it persists across the boundary's re-renders).
    pub fn wrap(errored: Signal<bool>) -> Self {
        Self { errored }
    }

    /// Whether this boundary has caught an error (reactive — the boundary reads this
    /// to decide fallback vs content).
    pub fn errored(&self) -> bool {
        self.errored.get()
    }

    /// Clear the error so the boundary re-tries its content (a "retry" affordance a
    /// fallback can offer). Reads the handle from context and calls this.
    pub fn reset(&self) {
        self.errored.set(false);
    }

    /// Trip the boundary (idempotent) — called by the reconciler on a caught panic.
    fn trip(self) {
        if !self.errored.peek() {
            self.errored.set(true);
        }
    }
}

/// Trip the nearest enclosing error boundary, if any. Returns whether one was found —
/// the reconciler propagates the panic when there is no boundary in scope (so a panic
/// with no boundary behaves exactly as before).
pub(crate) fn trip_nearest_error_boundary() -> bool {
    match consume_context::<ErrorBoundaryHandle>() {
        Some(handle) => {
            handle.trip();
            true
        }
        None => false,
    }
}

/// What the app does with a component render panic that no `error_boundary` caught.
type RenderPanicHandler = std::rc::Rc<dyn Fn(&str)>;

thread_local! {
    static RENDER_PANIC_HANDLER: std::cell::RefCell<Option<RenderPanicHandler>> =
        const { std::cell::RefCell::new(None) };
}

/// Keep the app alive on an uncaught component render panic (Flutter's release-mode
/// `ErrorWidget` behaviour): with a handler installed, a render panic that no
/// `error_boundary` caught is passed to `handler` (with the panic message) and the
/// failing component renders **nothing** this frame instead of unwinding out of the
/// frame — it re-renders normally the next time it is marked dirty. The process panic
/// hook has already run (log the backtrace there). Per UI thread;
/// [`clear_render_panic_handler`] restores the default (re-raise).
pub fn set_render_panic_handler(handler: impl Fn(&str) + 'static) {
    let h: RenderPanicHandler = std::rc::Rc::new(handler);
    RENDER_PANIC_HANDLER.with(|c| *c.borrow_mut() = Some(h));
}

/// Remove the [`set_render_panic_handler`] handler: uncaught render panics re-raise again.
pub fn clear_render_panic_handler() {
    RENDER_PANIC_HANDLER.with(|c| *c.borrow_mut() = None);
}

/// Report an uncaught render panic to the installed handler; `false` when none is set.
pub(crate) fn handle_uncaught_render_panic(payload: &(dyn std::any::Any + Send)) -> bool {
    let Some(h) = RENDER_PANIC_HANDLER.with(|c| c.borrow().clone()) else {
        return false;
    };
    let msg = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a component render panicked".to_string());
    h(&msg);
    true
}
