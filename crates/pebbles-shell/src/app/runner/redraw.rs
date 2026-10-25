//! Per-window redraw bookkeeping, so no window can starve another of frames.
//!
//! winit on Windows dispatches exactly one `RedrawRequested` per loop turn (it interrupts
//! message dispatch after each), and `WM_PAINT` is synthesized for whichever invalidated
//! window the OS finds first. A window that re-requests a frame from its own
//! `RedrawRequested` (a spinner) is invalid again on every turn and can win every turn, so
//! another window with a pending request never gets its `WM_PAINT` (the main window sat at
//! 0 fps for 27 s next to a 60 fps spinner window). Every runner frame request goes through
//! [`request`], which records the window; after a `RedrawRequested`, the runner draws every
//! other window still recorded here ([`drain_starved`]) in the same turn.

use std::cell::RefCell;
use std::collections::HashSet;

use winit::window::{Window, WindowId};

thread_local! {
    /// Windows with a frame requested and not yet drawn.
    static PENDING: RefCell<HashSet<WindowId>> = RefCell::new(HashSet::new());
    /// Windows drawn on another window's turn whose own `RedrawRequested` is still queued.
    static SERVED: RefCell<HashSet<WindowId>> = RefCell::new(HashSet::new());
}

/// Whether a `RedrawRequested` also serves other windows' pending frames. Only winit on
/// Windows delivers one redraw per turn; elsewhere every pending redraw arrives each turn.
pub(super) const SERVE_STARVED: bool = cfg!(target_os = "windows");

/// Request a frame for `window` (use instead of `Window::request_redraw`).
pub(super) fn request(window: &Window) {
    PENDING.with(|p| p.borrow_mut().insert(window.id()));
    window.request_redraw();
}

/// A `RedrawRequested` for `id` arrived: `false` means skip it — the window was already
/// drawn on another window's turn and has requested nothing since.
pub(super) fn begin(id: WindowId) -> bool {
    let wanted = PENDING.with(|p| p.borrow_mut().remove(&id));
    let served = SERVED.with(|s| s.borrow_mut().remove(&id));
    wanted || !served
}

/// Every window other than `current` that still has a frame pending, each marked served (its
/// own queued `RedrawRequested` is then skipped unless it requests again).
pub(super) fn drain_starved(current: WindowId) -> Vec<WindowId> {
    let ids: Vec<WindowId> = PENDING.with(|p| {
        let mut p = p.borrow_mut();
        let ids: Vec<WindowId> = p.iter().copied().filter(|id| *id != current).collect();
        for id in &ids {
            p.remove(id);
        }
        ids
    });
    SERVED.with(|s| s.borrow_mut().extend(ids.iter().copied()));
    ids
}

/// Forget a closed window.
pub(super) fn forget(id: WindowId) {
    PENDING.with(|p| p.borrow_mut().remove(&id));
    SERVED.with(|s| s.borrow_mut().remove(&id));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn served_window_skips_its_queued_redraw_unless_requested_again() {
        let a = WindowId::dummy();
        assert!(begin(a), "an OS-originated paint (nothing recorded) is drawn");
        PENDING.with(|p| p.borrow_mut().insert(a));
        let b = drain_starved(a);
        assert!(b.is_empty(), "the current window is not its own starved window");
        forget(a);
        SERVED.with(|s| s.borrow_mut().insert(a));
        assert!(!begin(a), "already drawn on another turn and nothing new requested");
        SERVED.with(|s| s.borrow_mut().insert(a));
        PENDING.with(|p| p.borrow_mut().insert(a));
        assert!(begin(a), "requested again after being served");
        forget(a);
    }
}
