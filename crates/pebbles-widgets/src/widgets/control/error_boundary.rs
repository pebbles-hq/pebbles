//! `error_boundary` — SolidJS `<ErrorBoundary>`.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;

use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Component, ErrorBoundaryHandle, component_props, create_signal, provide_context};

struct ErrorBoundaryProps {
    content: Rc<dyn Fn() -> AnyWidget>,
    fallback: Rc<dyn Fn() -> AnyWidget>,
}

fn render(p: &ErrorBoundaryProps) -> AnyWidget {
    // One "errored" signal per boundary instance (a hook — persists across the
    // boundary's own re-renders). Provide a handle into context so the reconciler can
    // route a descendant render panic to THIS boundary; a fallback can `reset()` it.
    let errored = create_signal(false);
    let handle = ErrorBoundaryHandle::wrap(errored);
    provide_context(handle);

    if handle.errored() {
        (p.fallback)()
    } else {
        // Build-time guard: a panic while constructing the content subtree flips the
        // boundary and shows the fallback this frame too.
        match catch_unwind(AssertUnwindSafe(|| (p.content)())) {
            Ok(widget) => widget,
            Err(_) => {
                errored.set(true);
                (p.fallback)()
            }
        }
    }
}

/// Contain errors in a subtree and show `fallback` instead of crashing the app
/// (SolidJS `<ErrorBoundary>`).
///
/// ```ignore
/// error_boundary(|| risky_view(data), || text("Something went wrong"))
/// ```
///
/// Catches **both** kinds of failure:
/// - a panic while *building* the content subtree (e.g. an `unwrap` on bad data), and
/// - a panic inside a *descendant component's render* during reconcile — the
///   reconciler routes it to the nearest boundary, which re-renders to its fallback.
///
/// The boundary stays in its fallback state until reset. A retry affordance can clear
/// it by reading the handle from context and calling
/// [`reset`](pebbles_core::ErrorBoundaryHandle::reset):
///
/// ```ignore
/// if let Some(b) = consume_context::<ErrorBoundaryHandle>() {
///     button("Retry").on_pressed(move || b.reset());
/// }
/// ```
///
/// (Under a `panic = "abort"` build there is nothing to catch — the boundary is inert,
/// as with any `catch_unwind`.)
pub fn error_boundary<Wc, Wf>(
    content: impl Fn() -> Wc + 'static,
    fallback: impl Fn() -> Wf + 'static,
) -> Component
where
    Wc: IntoWidget,
    Wf: IntoWidget,
{
    let content: Rc<dyn Fn() -> AnyWidget> = Rc::new(move || content().into_widget());
    let fallback: Rc<dyn Fn() -> AnyWidget> = Rc::new(move || fallback().into_widget());
    component_props(render, ErrorBoundaryProps { content, fallback })
}
