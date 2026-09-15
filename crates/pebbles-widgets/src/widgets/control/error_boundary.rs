//! `error_boundary` — SolidJS `<ErrorBoundary>` (build-time).

use std::panic::{AssertUnwindSafe, catch_unwind};

use pebbles_core::widget::{AnyWidget, IntoWidget};

/// Build `content`; if constructing it **panics**, show `fallback` instead (SolidJS
/// `<ErrorBoundary>`).
///
/// ```ignore
/// error_boundary(|| risky_view(data), || text("Something went wrong"))
/// ```
///
/// **Scope.** This catches panics raised while *building* the subtree — e.g. an
/// `unwrap` on malformed data while composing the view. It runs in the enclosing
/// render, so a caught panic leaves no dangling reactive scope. Panics inside a nested
/// component's *own later re-render* are that component's separate reconcile pass and
/// are not caught here; a full render-time boundary needs reconciler integration
/// (future work). The panic is still reported through the process panic hook.
pub fn error_boundary<C, F, W1, W2>(content: C, fallback: F) -> AnyWidget
where
    C: FnOnce() -> W1,
    F: FnOnce() -> W2,
    W1: IntoWidget,
    W2: IntoWidget,
{
    match catch_unwind(AssertUnwindSafe(|| content().into_widget())) {
        Ok(widget) => widget,
        Err(_) => fallback().into_widget(),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::error_boundary;
    use crate::widgets::{gap_h, text};
    use pebbles_core::widget::AnyWidget;

    #[test]
    fn happy_path_returns_content() {
        let used_fallback = Rc::new(Cell::new(false));
        let f = used_fallback.clone();
        let _ = error_boundary(
            || text("ok"),
            move || {
                f.set(true);
                gap_h(0.0)
            },
        );
        assert!(!used_fallback.get(), "content built fine — fallback not used");
    }

    #[test]
    fn panicking_content_falls_back() {
        fn boom() -> AnyWidget {
            panic!("boom")
        }
        let used_fallback = Rc::new(Cell::new(false));
        let f = used_fallback.clone();
        // Silence the panic hook for the intentional panic below.
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let _ = error_boundary(boom, move || {
            f.set(true);
            gap_h(0.0)
        });
        std::panic::set_hook(prev);
        assert!(used_fallback.get(), "a panic during build shows the fallback");
    }
}
