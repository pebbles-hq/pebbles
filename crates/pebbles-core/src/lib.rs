//! # pebbles-core
//!
//! The runtime layer — the framework engine every widget is built on, with **no**
//! catalog of its own. It owns three things:
//!
//! * **Reactivity** — the Solid-style [`Signal`]/[`create_memo`]/[`create_effect`]/
//!   [`create_store`] primitives. State is a signal you read and write directly;
//!   reads inside a component subscribe, writes re-render only the readers.
//! * **The widget contract** — [`Widget`], the [`component`](fn@component)/[`component_props`]
//!   function-component adapter (the primary way to author UI), the [`RenderWidget`]/
//!   [`ParentDataWidget`] render layer, and the [`IntoWidget`]/[`AnyWidget`] boxing
//!   layer.
//! * **The reconciling engine** — [`Ui`], the element arena keyed by [`ElementId`]
//!   that turns a widget tree into a [`pebbles_render`] tree each frame, plus the
//!   [`focus`] system and the [`Callback`] handle.
//!
//! The widget catalog lives one layer up in `pebbles-widgets`; the GPU/windowing
//! shell lives in `pebbles-shell`. This crate depends only on `pebbles-foundation`
//! and `pebbles-render`.

pub mod animation;
pub mod bounds;
pub mod clipboard;
pub mod component;
pub mod context;
pub mod element;
pub mod focus;
pub mod ipc;
pub mod key;
pub mod keyboard;
/// Re-export the diagnostic log — it lives in `pebbles-foundation` (the lowest
/// crate) so every layer, including the render engine below core, can log to one
/// stream. `pebbles_core::log` stays a valid path.
pub use pebbles_foundation::log;
pub mod reactive;
// `reactive_stats` keeps its flat public path (the shell reads it as
// `pebbles_core::reactive_stats`); it now lives inside the reactive module.
pub use reactive::stats as reactive_stats;
pub mod scroll;
pub mod shortcuts;
pub mod task;
pub mod widget;

pub use animation::{
    Curve, Spring, Transition, animate_spring, animate_to, animate_to_with, animated, animated_spring,
    animated_with, clear_timeout, create_loop, create_loop_while, create_timeout, set_timeout, transition,
};
pub use bounds::use_bounds;
pub use component::{Component, Element, component, component_props};
pub use context::{Callback, IntoCallback, action, action_event};
pub use element::{ElementId, Ui};
pub use focus::{FocusNode, ScopeTag, create_focus, create_focus_scope, editor_is_focused, registered_nodes};
pub use ipc::{Channel, channel};
pub use key::Key;
pub use keyboard::{KeyInput, Motion};
pub use reactive::{
    Signal, Store, consume_context, create_cleanup, create_effect, create_memo, create_memo_with,
    create_root_signal, create_signal, create_store, on, on_defer, owner_id, provide_context, untrack,
};
pub use shortcuts::{Mods, ShortcutKey, create_shortcut, create_shortcut_if};
pub use task::{Resource, create_resource, spawn};
#[cfg(target_family = "wasm")]
pub use task::spawn_local_future;
#[cfg(feature = "tokio")]
pub use task::{create_resource_future, spawn_future};
pub use widget::{AnyWidget, IntoChildren, IntoWidget, ParentDataWidget, RenderWidget, Widget};

// Debug-only census accessors (lifecycle soak tripwire — performance-standards.md E6c).
#[cfg(debug_assertions)]
pub use animation::{census_loops, census_timeouts};
#[cfg(debug_assertions)]
pub use focus::census_registrations;
#[cfg(debug_assertions)]
pub use reactive::{census_cleanups, census_memos, census_pending, census_signals, census_subscriptions};
#[cfg(debug_assertions)]
pub use scroll::census_handlers;
