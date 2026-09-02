//! A registry that lets the shell drive **controlled** scroll views (whose offset
//! is a reactive signal) — the substrate for virtualized lists.
//!
//! A controlled viewport installs a handler keyed by its stable id; the shell's
//! wheel + scrollbar-drag dispatch look it up and call it with a [`ScrollTo`]. The
//! handler (owned by the widget) clamps and writes the offset signal, which
//! re-renders the list so only the newly-visible items are built.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// How to move a controlled viewport.
#[derive(Clone, Copy, Debug)]
pub enum ScrollTo {
    /// Add a delta (wheel).
    By(f64),
    /// Jump to a fraction `0.0..=1.0` of the scrollable range (scrollbar).
    ToFraction(f64),
}

thread_local! {
    static HANDLERS: RefCell<HashMap<u64, Rc<dyn Fn(ScrollTo)>>> = RefCell::new(HashMap::new());
}

/// Install (or replace) the scroll handler for viewport `id`.
pub fn install(id: u64, handler: Rc<dyn Fn(ScrollTo)>) {
    HANDLERS.with(|h| {
        h.borrow_mut().insert(id, handler);
    });
}

/// Remove a viewport's handler (on unmount).
pub fn clear(id: u64) {
    HANDLERS.with(|h| {
        h.borrow_mut().remove(&id);
    });
}

/// Drive viewport `id`. Returns whether a handler was found.
pub fn dispatch(id: u64, to: ScrollTo) -> bool {
    let handler = HANDLERS.with(|h| h.borrow().get(&id).cloned());
    match handler {
        Some(h) => {
            h(to);
            true
        }
        None => false,
    }
}

/// Number of live scroll handlers (debug-only).
#[cfg(debug_assertions)]
pub fn census_handlers() -> usize {
    HANDLERS.with(|h| h.borrow().len())
}
