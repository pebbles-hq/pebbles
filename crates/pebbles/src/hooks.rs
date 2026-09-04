//! Every Pebbles hook, in one place.
//!
//! Hooks are plain functions you call from a component body. They're defined
//! across the runtime and catalog crates (reactivity in `pebbles-core`,
//! controllers next to the widgets that use them), which makes them hard to
//! discover — this module is the single index. Everything here is also in
//! [`crate::prelude`]; import either.
//!
//! **The hooks rule.** A hook allocates a slot in the component's hook order on
//! first render, so call them unconditionally at the top of a component body —
//! never inside an `if`, a loop, or a closure. (Reading a signal is *not* a
//! hook; only the `create_*` / `use_*` functions are.) The one exception is
//! [`create_root_signal`], which owns its signal at the app root and never
//! counts against the order.
//!
//! ### State
//! * [`create_signal`] — local reactive state; reading it inside a component
//!   subscribes that component.
//! * [`create_store`] — a struct of fields you can subscribe to individually.
//! * [`create_root_signal`] — app-scope state, owned by the root rather than the
//!   calling component (how the theme, overlay host and focus registries
//!   initialize lazily without binding to whoever touched them first).
//!
//! ### Derived values
//! * [`create_memo`] — a cached computation that recomputes only when a
//!   dependency it actually read changes, and only when something reads it.
//! * [`create_memo_with`] — the same, with a custom equality policy (e.g.
//!   `Rc::ptr_eq` to cut without a deep compare; drops the `PartialEq` bound).
//!
//! ### Effects
//! * [`create_effect`] — run a side effect after render, re-running when its
//!   tracked reads change.
//! * [`on`] / [`on_defer`] — explicit-dependency effects: track only `deps` and
//!   run the body untracked (`on_defer` skips the mount run).
//! * [`create_cleanup`] — run teardown when the owning component unmounts.
//!
//! ### Time & async
//! * [`create_timeout`] — a one-shot timer, keyed so re-registering replaces the
//!   pending fire (that *is* debounce).
//! * [`create_loop`] / [`create_loop_while`] — a repeating phase signal; the
//!   `_while` form ticks only while its condition holds, so an idle screen
//!   doesn't keep the frame loop alive.
//! * [`create_resource`] — load async data into a `Resource<T>` with
//!   loading/ready/error states.
//! * `create_resource_future` — the same over a `Future` (needs the `tokio`
//!   feature).
//!
//! ### Focus & input
//! * [`create_focus`] — a focus node you can query and request focus on.
//! * [`create_focus_scope`] — a focus trap/group (dialogs, menus).
//! * [`create_shortcut`] / [`create_shortcut_if`] — bind a key combination,
//!   conditionally for the `_if` form.
//!
//! ### Measurement & controllers
//! * [`use_bounds`] — the laid-out rect of a widget, after layout.
//! * [`use_scroll_controller`] — drive/observe a scroll view.
//! * [`use_carousel_controller`] — drive/observe a carousel.

// --- state -----------------------------------------------------------------
pub use pebbles_core::{create_root_signal, create_signal, create_store};

// --- derived ---------------------------------------------------------------
pub use pebbles_core::{create_memo, create_memo_with};

// --- effects ---------------------------------------------------------------
pub use pebbles_core::{create_cleanup, create_effect, on, on_defer};

// --- time & async ----------------------------------------------------------
pub use pebbles_core::{create_loop, create_loop_while, create_resource, create_timeout};
#[cfg(feature = "tokio")]
pub use pebbles_core::create_resource_future;

// --- focus & input ---------------------------------------------------------
pub use pebbles_core::{create_focus, create_focus_scope, create_shortcut, create_shortcut_if};

// --- measurement & controllers ---------------------------------------------
pub use pebbles_core::use_bounds;
pub use pebbles_widgets::{use_carousel_controller, use_scroll_controller};
