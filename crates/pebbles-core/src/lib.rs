//! # pebbles-core
//!
//! The runtime layer — the framework engine every widget is built on, with **no**
//! catalog of its own. It owns three things:
//!
//! * **The widget contract** — [`Widget`] and its categories ([`StatelessWidget`],
//!   [`StatefulWidget`] + [`State`], [`RenderWidget`], [`ParentDataWidget`]), the
//!   [`IntoWidget`]/[`AnyWidget`] boxing layer, and the declaration macros
//!   ([`render_widget!`], [`stateless_widget!`], [`children!`], …).
//! * **The reconciling engine** — [`Ui`], the element arena keyed by [`ElementId`]
//!   that turns a widget tree into a [`pebbles_render`] tree each frame, plus the
//!   [`BuildContext`]/[`Callback`] build-time handles and the [`focus`] system.
//! * **Reactivity** — the SolidJS-style [`Signal`]/[`create_memo`]/[`create_effect`]/
//!   [`create_store`] primitives and the [`component`] function-component adapter.
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
pub mod state;
pub mod widget;

pub use animation::{animate_to, animated, create_loop};
pub use component::{Component, Element, component, component_props};
pub use context::{BuildContext, Callback, action, action_event};
pub use element::{ElementId, Ui};
pub use focus::{FocusNode, create_focus};
pub use ipc::{Channel, channel};
pub use key::Key;
pub use keyboard::{KeyInput, Motion};
pub use reactive::{
    Signal, Store, create_cleanup, create_effect, create_memo, create_signal, create_store,
    owner_id,
};
pub use state::State;
pub use widget::{
    AnyWidget, IntoWidget, ParentDataWidget, RenderWidget, StatefulWidget, StatelessWidget, Widget,
    WidgetExt,
};
