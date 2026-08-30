//! # pebbles-core
//!
//! The runtime layer — the framework engine every widget is built on, with **no**
//! catalog of its own. It owns three things:
//!
//! * **Reactivity** — the Solid-style [`Signal`]/[`create_memo`]/[`create_effect`]/
//!   [`create_store`] primitives. State is a signal you read and write directly;
//!   reads inside a component subscribe, writes re-render only the readers.
//! * **The widget contract** — [`Widget`], the [`component`]/[`component_props`]
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
pub mod clipboard;
pub mod component;
pub mod context;
pub mod element;
pub mod focus;
pub mod ipc;
pub mod key;
pub mod keyboard;
pub mod reactive;
pub mod scroll;
pub mod task;
pub mod widget;

pub use animation::{animate_to, animated, create_loop, create_timeout};
pub use component::{Component, Element, component, component_props};
pub use context::{Callback, IntoCallback, action, action_event};
pub use element::{ElementId, Ui};
pub use focus::{FocusNode, create_focus};
pub use ipc::{Channel, channel};
pub use key::Key;
pub use keyboard::{KeyInput, Motion};
pub use reactive::{
    Signal, Store, create_cleanup, create_effect, create_memo, create_root_signal, create_signal,
    create_store, owner_id,
};
pub use task::{Resource, create_resource, spawn};
pub use widget::{
    AnyWidget, IntoChildren, IntoWidget, ParentDataWidget, RenderWidget, Widget, WidgetExt,
};
