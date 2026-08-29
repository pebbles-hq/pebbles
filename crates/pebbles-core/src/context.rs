//! [`Callback`] — the event handler type.
//!
//! In the SolidJS-style model a callback is a **plain closure** that captures
//! signals: `on_pressed(action(move || count.update(|c| *c += 1)))`. When it fires,
//! the framework simply calls it; the signal writes inside schedule the affected
//! components for re-render. Build [`Callback`]s with the free function
//! [`action`].
//!
//! (The `Targeted` variant is transitional scaffolding for the class-based widgets
//! still being migrated; it is removed once the pivot completes.)

use std::any::Any;
use std::rc::Rc;

use pebbles_render::PointerEvent;

use crate::element::ElementId;
use crate::state::State;

/// An event handler. Cloneable (refcounted) so it can travel into a render object.
#[derive(Clone)]
pub enum Callback {
    /// A plain reactive closure (the common case).
    Plain(Rc<dyn Fn()>),
    /// A handler that receives the [`PointerEvent`] (position, button) — for
    /// `on_tap_down`, `on_tap_up`, etc.
    Event(Rc<dyn Fn(PointerEvent)>),
    /// Legacy: a type-erased mutation bound to an element's `State`.
    Targeted { target: ElementId, action: Rc<dyn Fn(&mut dyn Any)> },
}

/// Wrap a plain closure as a [`Callback`] — the idiomatic reactive handler.
///
/// ```ignore
/// button("+").on_pressed(action(move || count.update(|c| *c += 1)))
/// ```
pub fn action(f: impl Fn() + 'static) -> Callback {
    Callback::Plain(Rc::new(f))
}

/// Wrap a closure that receives the [`PointerEvent`] (position + button).
///
/// ```ignore
/// gesture.on_tap_down(action_event(move |e| println!("tapped at {:?}", e.position)))
/// ```
pub fn action_event(f: impl Fn(PointerEvent) + 'static) -> Callback {
    Callback::Event(Rc::new(f))
}

/// Handle passed to legacy `build` methods (being removed). It exposes the owning
/// element's id and the [`callback`](BuildContext::callback) factory.
pub struct BuildContext {
    pub(crate) id: ElementId,
}

impl BuildContext {
    pub(crate) fn new(id: ElementId) -> Self {
        BuildContext { id }
    }

    /// This element's id.
    pub fn element_id(&self) -> ElementId {
        self.id
    }

    /// Legacy: create a `State`-mutating callback (transitional).
    pub fn callback<S, F>(&self, f: F) -> Callback
    where
        S: State,
        F: Fn(&mut S) + 'static,
    {
        Callback::Targeted {
            target: self.id,
            action: Rc::new(move |any: &mut dyn Any| {
                if let Some(state) = any.downcast_mut::<S>() {
                    f(state);
                }
            }),
        }
    }
}
