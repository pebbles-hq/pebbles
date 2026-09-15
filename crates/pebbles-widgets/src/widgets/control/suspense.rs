//! `suspense` — SolidJS `<Suspense>` (explicit-predicate form).

use std::rc::Rc;

use pebbles_core::Component;
use pebbles_core::component_props;
use pebbles_core::widget::{AnyWidget, IntoWidget};

struct SuspenseProps {
    loading: Rc<dyn Fn() -> bool>,
    fallback: Rc<dyn Fn() -> AnyWidget>,
    content: Rc<dyn Fn() -> AnyWidget>,
}

fn render(p: &SuspenseProps) -> AnyWidget {
    if (p.loading)() { (p.fallback)() } else { (p.content)() }
}

/// Show `fallback` while `loading()` is true, else `content` (SolidJS `<Suspense>`).
///
/// `loading` is read reactively, so the swap happens on its own when the resources it
/// checks resolve:
///
/// ```ignore
/// suspense(
///     move || user.get().is_loading() || posts.get().is_loading(),
///     || spinner(20.0),
///     move || profile(user.get(), posts.get()),
/// )
/// ```
///
/// SolidJS auto-tracks which resources a subtree read; Pebbles takes the predicate
/// explicitly — the Rust-idiomatic equivalent, and it coordinates any number of
/// [`Resource`](pebbles_core::Resource)s by `||`-ing their `is_loading()`.
pub fn suspense<Wf, Wc>(
    loading: impl Fn() -> bool + 'static,
    fallback: impl Fn() -> Wf + 'static,
    content: impl Fn() -> Wc + 'static,
) -> Component
where
    Wf: IntoWidget,
    Wc: IntoWidget,
{
    let fallback: Rc<dyn Fn() -> AnyWidget> = Rc::new(move || fallback().into_widget());
    let content: Rc<dyn Fn() -> AnyWidget> = Rc::new(move || content().into_widget());
    component_props(render, SuspenseProps { loading: Rc::new(loading), fallback, content })
}
