//! `use_bounds()` — a component reads its own laid-out window-space rect, reactively,
//! **one frame behind**. Widgets can't see the render tree at render time (layout hasn't
//! run yet), so the shell publishes each interested component's rect after every layout
//! and this hook returns the last published value + subscribes to changes.
//!
//! Backs tooltip `show_on_focus` positioning (C2) and any widget that must anchor to its
//! own geometry. The registry is keyed by `(window, element-ffi-id)`; the shell drives
//! [`wanted_bounds`] → [`publish_bounds`], and GCs unmounted keys via [`forget_bounds`].

use std::cell::RefCell;
use std::collections::HashMap;

use pebbles_foundation::Rect;

use crate::reactive::{Signal, create_cleanup, create_root_signal, current_window, dispose_root_signal, owner_id};

thread_local! {
    static BOUNDS: RefCell<HashMap<(u32, u64), Signal<Rect>>> = RefCell::new(HashMap::new());
    static FOCUS_BOUNDS: RefCell<Option<Signal<Option<Rect>>>> = const { RefCell::new(None) };
}

fn focus_bounds_signal() -> Signal<Option<Rect>> {
    FOCUS_BOUNDS.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(None)))
}

/// The window-space rect of the currently focused element (reactive), or `None`. Used by
/// tooltip `show_on_focus` to detect that its trigger holds focus.
pub fn focus_bounds() -> Option<Rect> {
    focus_bounds_signal().get()
}

/// Shell-only: publish the focused element's rect each frame (no-op when unchanged).
pub fn set_focus_bounds(rect: Option<Rect>) {
    let sig = focus_bounds_signal();
    if sig.peek() != rect {
        sig.set(rect);
    }
}

/// Component hook: this component's own laid-out rect in window space, from the last
/// frame's layout. `Rect::ZERO` until it has been laid out once. Reading it subscribes,
/// so the component re-renders when its rect changes.
pub fn use_bounds() -> Rect {
    let window = current_window();
    let Some(id) = owner_id() else {
        return Rect::ZERO;
    };
    // A stable root signal per (window, element) — not a positional hook, so calling
    // `use_bounds` conditionally is still safe.
    let sig = BOUNDS.with(|b| {
        *b.borrow_mut().entry((window, id)).or_insert_with(|| create_root_signal(Rect::ZERO))
    });
    // Free the registry entry AND its root signal when this component unmounts.
    // Root signals live outside the hook arena, so without this every remount
    // minted a fresh immortal signal (the E6c lifecycle-soak tripwire).
    create_cleanup(move || {
        if let Some(s) = BOUNDS.with(|b| b.borrow_mut().remove(&(window, id))) {
            dispose_root_signal(s);
        }
    });
    sig.get()
}

/// Shell-only: the `(window, source)` keys currently wanting bounds, so the shell can
/// look each up in the render tree after layout.
pub fn wanted_bounds() -> Vec<(u32, u64)> {
    BOUNDS.with(|b| b.borrow().keys().copied().collect())
}

/// Shell-only: publish `rect` for `(window, source)` (no-op when unchanged, so it never
/// spuriously re-renders).
pub fn publish_bounds(window: u32, source: u64, rect: Rect) {
    let sig = BOUNDS.with(|b| b.borrow().get(&(window, source)).copied());
    if let Some(sig) = sig
        && sig.peek() != rect
    {
        sig.set(rect);
    }
}

/// Shell-only: drop a key whose element is gone from the tree (unmounted) —
/// frees the backing root signal too (idempotent with the unmount cleanup).
pub fn forget_bounds(window: u32, source: u64) {
    if let Some(s) = BOUNDS.with(|b| b.borrow_mut().remove(&(window, source))) {
        dispose_root_signal(s);
    }
}
