//! Function components — the SolidJS-style unit of composition. A component is a
//! function that reads signals and returns a widget tree. No structs-as-widgets,
//! no traits, no `State`.
//!
//! Components return `impl IntoWidget` — so the body's final expression can be any
//! widget (`center(...)`, a `Container`, …) with **no `.into_widget()`**:
//!
//! ```ignore
//! fn counter() -> impl IntoWidget {
//!     let count = create_signal(0);
//!     center(column(children![
//!         text(format!("{}", count.get())).size(64.0),
//!         button("+").on_pressed(action(move || count.update(|c| *c += 1))),
//!     ]))
//! }
//! ```
//!
//! Two forms: no props — `fn() -> impl IntoWidget` via [`component`]; with props —
//! `fn(&P) -> impl IntoWidget` via [`component_props`].
//!
//! # When the body re-runs
//!
//! A component body is **not** guaranteed to run only once. It runs on mount, and
//! re-runs when:
//!
//! - a signal it read during its last run changes (it is marked dirty), or
//! - its parent re-renders and hands back a **new** component instance — a fresh
//!   [`component`]/[`component_props`] call, even with equal props (there is no
//!   props-equality bailout), or
//! - its parent re-renders and a render-time context it can see changed (an
//!   ancestor's [`provide_context`](crate::reactive::provide_context) produced a new
//!   value).
//!
//! It does **not** re-run when its parent re-renders but hands back the *same*
//! instance (a clone of the same `Component`, e.g. a `child` stored in props and
//! returned again, as `OverlayHost` does) with nothing above it changed — the
//! existing subtree is kept, like Flutter's identical-widget and React's
//! same-element bailouts.
//!
//! So keep state that must survive re-runs in [`create_signal`](crate::reactive::create_signal)
//! (a component-local hook, preserved across re-runs by call order and disposed on
//! unmount). A [`create_root_signal`](crate::reactive::create_root_signal) called in a
//! body creates a **new** app-owned signal on every run — it is for lazily
//! initialized global state, not per-component state.

use std::any::Any;
use std::rc::Rc;

use crate::widget::{AnyWidget, IntoWidget, Widget};

/// The conventional return type of a component. `impl IntoWidget` is preferred (no
/// boxing), but this alias is available when a concrete type is needed.
pub type Element = AnyWidget;

/// A function component: an identity plus a thunk that (re)builds its subtree.
#[derive(Clone)]
pub struct Component {
    pub(crate) id: usize,
    pub(crate) render: Rc<dyn Fn() -> AnyWidget>,
}

/// Wrap a no-props component `fn() -> impl IntoWidget`.
pub fn component<W: IntoWidget + 'static>(func: fn() -> W) -> Component {
    Component { id: func as usize, render: Rc::new(move || func().into_widget()) }
}

/// Wrap a component `fn(&P) -> impl IntoWidget` with its props. All instances of the
/// same component function share identity (so their element/state is reused
/// positionally).
pub fn component_props<P: 'static, W: IntoWidget + 'static>(func: fn(&P) -> W, props: P) -> Component {
    Component { id: func as usize, render: Rc::new(move || func(&props).into_widget()) }
}

impl Widget for Component {
    fn debug_name(&self) -> &'static str {
        "Component"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_box(&self) -> AnyWidget {
        Box::new(self.clone())
    }
    fn as_component(&self) -> Option<(usize, Rc<dyn Fn() -> Element>)> {
        Some((self.id, self.render.clone()))
    }
}
