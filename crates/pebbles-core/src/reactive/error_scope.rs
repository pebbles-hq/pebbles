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
    fn trip(&self) {
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
