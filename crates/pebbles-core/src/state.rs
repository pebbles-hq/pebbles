//! The [`State`] trait — the mutable, long-lived companion to a `StatefulWidget`.
//!
//! State survives rebuilds. It is created once (`create_state` + [`State::init_state`]),
//! rebuilt whenever it is marked dirty (via a [`Callback`](crate::Callback)) or its
//! widget updates, and disposed when the element unmounts.

use std::any::Any;

use crate::context::BuildContext;
use crate::widget::{AnyWidget, Widget};

/// Mutable state for a stateful widget.
///
/// Implementors are stored as `Box<dyn State>` in the element tree. The `Any`
/// downcast (via [`State::as_any_mut`]) is how a [`Callback`](crate::Callback)
/// recovers the concrete type to run its mutation.
pub trait State: 'static {
    /// Describe the UI from the current state. Called on mount and after every
    /// change. Return exactly one widget (compose with `Column`, `Stack`, …).
    ///
    /// `widget` is the current widget configuration — downcast it with
    /// `widget.downcast_ref::<MyWidget>()` to read props (Flutter's `State.widget`).
    fn build(&mut self, widget: &dyn Widget, cx: &mut BuildContext) -> AnyWidget;

    /// Called once, immediately after the state is created and before the first
    /// `build`. Override to kick off timers, subscriptions, etc.
    fn init_state(&mut self, _widget: &dyn Widget, _cx: &mut BuildContext) {}

    /// Called when the owning widget was rebuilt with new configuration but the
    /// same type + key, so this state is being reused. `widget` is the new config.
    fn did_update_widget(&mut self, _widget: &dyn Widget, _cx: &mut BuildContext) {}

    /// Called just before the element unmounts. Override to release resources.
    fn dispose(&mut self) {}

    /// Recover the concrete state type for a callback mutation.
    fn as_any_mut(&mut self) -> &mut dyn Any;
}
