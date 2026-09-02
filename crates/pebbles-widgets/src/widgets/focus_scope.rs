//! [`FocusScope`] widget — wraps a subtree in a focus scope so Tab cycles **only
//! within it** (Flutter's `FocusScope`). Dialogs and sheets wrap their content in
//! one so keyboard traversal can't escape into the page behind them.
//!
//! Backed by [`pebbles_core::create_focus_scope`]: the widget is a function
//! component whose render provides the scope to its whole subtree via the
//! render-time context stack.

use pebbles_core::component_props;
use pebbles_core::widget::{AnyWidget, IntoWidget};

#[derive(Clone)]
struct Props {
    child: AnyWidget,
}

fn render(props: &Props) -> pebbles_core::Element {
    pebbles_core::create_focus_scope();
    props.child.clone()
}

/// Trap Tab-cycling within `child`: focusable widgets inside it form their own
/// cycle, so keyboard traversal never leaks out (and, while nothing inside is
/// focused, widgets outside the scope are what Tab visits).
pub fn focus_scope(child: impl pebbles_core::IntoWidget) -> pebbles_core::Element {
    component_props(render, Props { child: child.into_widget() }).into_widget()
}
