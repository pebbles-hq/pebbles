//! Background work bridged back to the reactive UI thread.
//!
//! The reactive runtime is single-threaded (signals aren't `Send`), so background
//! work runs on a plain `std::thread` and its result is handed back on the UI thread
//! via a shared slot that [`pump`] drains once per frame — the same proven pattern
//! `ImageView` used, generalized here.
//!
//! Two entry points:
//! * [`spawn`] — fire-and-forget: run `work` on a background thread, then call
//!   `on_done(result)` on the UI thread (so it can write signals) once it finishes.
//! * [`create_resource`] — the SolidJS-style async read: kick off `fetcher` once and
//!   get a `Signal<Resource<T>>` that starts [`Loading`](Resource::Loading) and flips
//!   to [`Ready`](Resource::Ready) when the value arrives. The fetcher runs ONCE per
//!   component mount — values it captures from props are frozen at creation. To
//!   refetch on a change, read that input as a signal INSIDE the fetcher, or
//!   re-mount the component.
//!
//! ```ignore
//! let user = create_resource(move || fetch_user(id)); // Signal<Resource<User>>
//! match user.get() {
//!     Resource::Loading => spinner(20.0).into_widget(),
//!     Resource::Ready(u) => text(u.name).into_widget(),
//! }
//! ```

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use crate::reactive::{Signal, create_effect, create_signal, request_frame};

/// The load state of a [`create_resource`] value: still running, or resolved. For
/// fallible work, use `Resource<Result<T, E>>` and match on the inner `Result`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resource<T> {
    /// The background work is still running.
    Loading,
    /// The work finished with this value.
    Ready(T),
}

impl<T> Resource<T> {
    /// The value if resolved, else `None`.
    pub fn value(&self) -> Option<&T> {
        match self {
            Resource::Ready(v) => Some(v),
            Resource::Loading => None,
        }
    }
    /// Whether the work is still in flight.
    pub fn is_loading(&self) -> bool {
        matches!(self, Resource::Loading)
    }
}

/// A completed-work poller: returns `true` once it has delivered its result (so
/// [`pump`] can drop it).
type Poller = Box<dyn FnMut() -> bool>;

thread_local! {
    // UI-thread only: background threads write to each task's `slot`, never this list.
    static PENDING: RefCell<Vec<Poller>> = const { RefCell::new(Vec::new()) };
}

/// Run `work` on a background thread, then deliver its result to `on_done` on the UI
/// thread (during the next [`pump`]), where it may write signals. Fire-and-forget:
/// the call returns immediately. For work with no UI result, pass `|_| {}`.
pub fn spawn<T, W, D>(work: W, on_done: D)
where
    T: Send + 'static,
    W: FnOnce() -> T + Send + 'static,
    D: FnOnce(T) + 'static,
{
    let slot: Arc<Mutex<Option<T>>> = Arc::new(Mutex::new(None));
    let write = slot.clone();
    std::thread::spawn(move || {
        let result = work();
        if let Ok(mut s) = write.lock() {
            *s = Some(result);
        }
    });
    // The result lands on a later frame; ask the shell to keep drawing until then.
    request_frame();
    let mut on_done = Some(on_done);
    PENDING.with(|p| {
        p.borrow_mut().push(Box::new(move || {
            match slot.lock().ok().and_then(|mut s| s.take()) {
                Some(v) => {
                    if let Some(cb) = on_done.take() {
                        cb(v);
                    }
                    true
                }
                None => false,
            }
        }));
    });
}

/// Kick off `fetcher` on a background thread **once** and return a signal that tracks
/// its state — [`Loading`](Resource::Loading) until it resolves, then
/// [`Ready`](Resource::Ready). Reading the signal in a component subscribes it, so the
/// view re-renders when the value arrives. Safe to call in a component body: the fetch
/// starts on mount (via an effect) and never re-fires on re-render.
pub fn create_resource<T, F>(fetcher: F) -> Signal<Resource<T>>
where
    T: Send + Clone + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let state = create_signal(Resource::Loading);
    // One-shot: the effect runs once on mount (it tracks nothing); the `take` guard
    // makes a second run — should one ever happen — a no-op instead of a re-fetch.
    let cell = std::rc::Rc::new(RefCell::new(Some(fetcher)));
    create_effect(move || {
        if let Some(f) = cell.borrow_mut().take() {
            spawn(f, move |v| state.set(Resource::Ready(v)));
        }
    });
    state
}

/// Drain finished background tasks and deliver their results (writing signals) on the
/// UI thread. Called once per frame by the shell. Returns whether any tasks are still
/// in flight (so the shell keeps requesting frames until they complete).
pub fn pump() -> bool {
    // Take the list so a task's `on_done` may itself `spawn` (re-entrant) without
    // aliasing the borrow; newly-spawned tasks land in the fresh list and are merged.
    let mut taken = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
    taken.retain_mut(|poll| !poll());
    PENDING.with(|p| {
        let mut cur = p.borrow_mut();
        taken.append(&mut cur);
        *cur = taken;
        !cur.is_empty()
    })
}
