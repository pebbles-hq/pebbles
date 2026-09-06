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

type ScrollHandler = Rc<dyn Fn(ScrollTo)>;
type IndexFn = Rc<dyn Fn(usize) -> f64>;

/// How to move a controlled viewport.
#[derive(Clone, Copy, Debug)]
pub enum ScrollTo {
    /// Add a delta (wheel).
    By(f64),
    /// Jump to a fraction `0.0..=1.0` of the scrollable range (scrollbar).
    ToFraction(f64),
}

thread_local! {
    static HANDLERS: RefCell<HashMap<u64, ScrollHandler>> = RefCell::new(HashMap::new());
    /// Per-viewport index→offset functions (auto-measured lists) so
    /// `scroll_to_index` resolves through the live extent cache.
    static INDEX_FNS: RefCell<HashMap<u64, IndexFn>> = RefCell::new(HashMap::new());
}

/// Install (or replace) the scroll handler for viewport `id`.
pub fn install(id: u64, handler: ScrollHandler) {
    HANDLERS.with(|h| {
        h.borrow_mut().insert(id, handler);
    });
}

/// Install (or replace) the index→offset function for viewport `id` — the
/// auto-measured list path of `ScrollController::scroll_to_index_auto`.
pub fn install_index(id: u64, f: IndexFn) {
    INDEX_FNS.with(|h| {
        h.borrow_mut().insert(id, f);
    });
}

/// The cached offset of `index` in viewport `id` (auto-measured lists).
pub fn index_of(id: u64, index: usize) -> Option<f64> {
    INDEX_FNS.with(|h| h.borrow().get(&id).map(|f| f(index)))
}

/// Remove a viewport's handler + index function (on unmount).
pub fn clear(id: u64) {
    HANDLERS.with(|h| {
        h.borrow_mut().remove(&id);
    });
    INDEX_FNS.with(|h| {
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
