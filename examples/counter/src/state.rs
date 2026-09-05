//! The app's **global state manager**.
//!
//! In Pebbles, global state is the *same* `create_signal` primitive as local state
//! — the trick is only *where* you create it. Made once at app scope (here, in a
//! `thread_local`), it outlives any single component and is shared everywhere with
//! **no prop-drilling**. Reading `count()` inside a component subscribes that
//! component; writing re-renders exactly the components that read it (SolidJS model).
//!
//! Pattern: expose the signal through a getter + a set of "actions" (the functions
//! below). Components call the actions and never touch the signal's internals — so
//! this file is the single source of truth for how the counter changes.

use std::cell::RefCell;

use pebbles::prelude::*;

thread_local! {
    // Created lazily on first use, then reused for the whole app lifetime.
    static COUNT: RefCell<Option<Signal<i32>>> = const { RefCell::new(None) };
}

/// The global counter signal (created once, on first access).
pub fn count() -> Signal<i32> {
    COUNT.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            // `create_root_signal` = a signal owned by the app, not the calling
            // component — so it survives re-renders and unmounts.
            *cell = Some(create_root_signal(0));
        }
        cell.unwrap()
    })
}

// --- actions: the only way the rest of the app mutates the counter ----------

pub fn increment() {
    count().update(|n| *n += 1);
}

pub fn decrement() {
    count().update(|n| *n -= 1);
}

pub fn reset() {
    count().set(0);
}
