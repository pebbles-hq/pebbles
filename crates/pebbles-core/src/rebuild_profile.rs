//! `PEBBLES_REBUILD_PROFILE=1`: time every dirty component's re-render (inclusive of
//! its reconciled subtree) and let the shell print the frame's costliest ones — finds a
//! signal read too high in the tree (a whole panel re-rendering for a hover) without a
//! sampling profiler. Off: one cached env check per dirty component.

use std::cell::RefCell;
use std::sync::OnceLock;
use std::time::Duration;

/// Whether the profiler is on (`PEBBLES_REBUILD_PROFILE=1`).
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("PEBBLES_REBUILD_PROFILE").is_ok_and(|v| v == "1"))
}

thread_local! {
    static ROWS: RefCell<Vec<(&'static str, usize, Duration)>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn record(name: &'static str, fn_addr: usize, took: Duration) {
    ROWS.with(|r| r.borrow_mut().push((name, fn_addr, took)));
}

/// This frame's dirty-component renders `(name, component fn address, inclusive time)`,
/// costliest first; clears the list. The address resolves with `nm` against
/// [`anchor`]'s (PIE-relative).
pub fn take() -> Vec<(&'static str, usize, Duration)> {
    let mut v = ROWS.with(|r| std::mem::take(&mut *r.borrow_mut()));
    v.sort_by(|a, b| b.2.cmp(&a.2));
    v
}

/// A fixed function whose address anchors component addresses for `nm` lookups.
#[inline(never)]
pub fn anchor() -> usize {
    anchor as usize
}
