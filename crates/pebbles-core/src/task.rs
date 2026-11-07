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

/// A thread-safe "a background result is ready" wake-up the shell installs with
/// [`set_waker`] (native: an event-loop proxy send). Called from the worker thread.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

/// One in-flight task: its poller, and whether it wakes the shell itself when done
/// (`false` means the shell must keep polling it every frame).
struct Pending {
    poll: Poller,
    wakes: bool,
}

thread_local! {
    // UI-thread only: background threads write to each task's `slot`, never this list.
    static PENDING: RefCell<Vec<Pending>> = const { RefCell::new(Vec::new()) };
    // The UI thread's waker (none in headless tests: those poll `pump`).
    static WAKER: RefCell<Option<Waker>> = const { RefCell::new(None) };
}

/// Install (or clear) this UI thread's [`Waker`]. With a waker, a finished background
/// task wakes the shell, so an idle window draws nothing while work is in flight; without
/// one, [`pump`] reports in-flight work and the shell keeps drawing frames to poll it.
pub fn set_waker(waker: Option<Waker>) {
    WAKER.with(|w| *w.borrow_mut() = waker);
}

#[cfg_attr(target_family = "wasm", allow(dead_code))] // web tasks are polled
fn current_waker() -> Option<Waker> {
    WAKER.with(|w| w.borrow().clone())
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
    spawn_after(std::time::Duration::ZERO, work, on_done);
}

/// [`spawn`], but `work` starts only after `delay` — for polling (a file watcher's next
/// tick) without parking a worker thread in `sleep` for the whole interval. One shared
/// timer thread holds the pending starts.
pub fn spawn_after<T, W, D>(delay: std::time::Duration, work: W, on_done: D)
where
    T: Send + 'static,
    W: FnOnce() -> T + Send + 'static,
    D: FnOnce(T) + 'static,
{
    let slot: Arc<Mutex<Option<T>>> = Arc::new(Mutex::new(None));
    let write = slot.clone();
    // Native: run `work` on a background thread.
    #[cfg(not(target_family = "wasm"))]
    let waker = current_waker();
    #[cfg(not(target_family = "wasm"))]
    let wakes = waker.is_some();
    #[cfg(not(target_family = "wasm"))]
    pool::submit(delay, Box::new(move || {
        let result = work();
        if let Ok(mut s) = write.lock() {
            *s = Some(result);
        }
        if let Some(wake) = waker {
            wake();
        }
    }));
    #[cfg(target_family = "wasm")]
    let _ = delay;
    #[cfg(target_family = "wasm")]
    let wakes = false;
    // Web: no OS threads (`std::thread::spawn` panics at runtime). Run `work` on the
    // microtask queue via `spawn_local` so it yields the current frame first (the
    // UI paints, THEN the task runs) instead of blocking inline; the result still
    // lands via `pump` on a later frame, preserving the reactive contract. See
    // documentations/web-support.md §4.3.
    #[cfg(target_family = "wasm")]
    wasm_bindgen_futures::spawn_local(async move {
        let result = work();
        if let Ok(mut s) = write.lock() {
            *s = Some(result);
        }
    });
    // The result lands on a later frame: with a waker the worker wakes the shell then;
    // without one, ask the shell to keep drawing until it arrives.
    if !wakes {
        request_frame();
    }
    let mut on_done = Some(on_done);
    let poll: Poller = Box::new(move || match slot.lock().ok().and_then(|mut s| s.take()) {
        Some(v) => {
            if let Some(cb) = on_done.take() {
                cb(v);
            }
            true
        }
        None => false,
    });
    PENDING.with(|p| p.borrow_mut().push(Pending { poll, wakes }));
}

/// Tasks spawned on this thread whose results are not yet delivered.
pub fn census_pending() -> usize {
    PENDING.with(|p| p.borrow().len())
}

/// Web-only async counterpart of [`spawn`]: drive `fut` on the browser microtask queue
/// (`wasm_bindgen_futures::spawn_local`) and deliver its output to `on_done` on the UI
/// thread through the same [`pump`] — so an async browser `fetch` can write signals and
/// re-render exactly like a background thread does on native. No tokio, no OS threads
/// (wasm has neither); the reactive runtime is single-threaded, so an `Rc` slot suffices.
#[cfg(target_family = "wasm")]
pub fn spawn_local_future<T, Fut, D>(fut: Fut, on_done: D)
where
    T: 'static,
    Fut: Future<Output = T> + 'static,
    D: FnOnce(T) + 'static,
{
    let slot: std::rc::Rc<RefCell<Option<T>>> = std::rc::Rc::new(RefCell::new(None));
    let write = slot.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let result = fut.await;
        *write.borrow_mut() = Some(result);
        // Wake the shell so `pump` runs and delivers even if the loop had gone idle.
        request_frame();
    });
    request_frame();
    let mut on_done = Some(on_done);
    let poll: Poller = Box::new(move || match slot.borrow_mut().take() {
        Some(v) => {
            if let Some(cb) = on_done.take() {
                cb(v);
            }
            true
        }
        None => false,
    });
    PENDING.with(|p| p.borrow_mut().push(Pending { poll, wakes: false }));
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

// ---------------------------------------------------------------------------
// F3 — optional tokio async (behind the `tokio` feature; default off)
// ---------------------------------------------------------------------------

#[cfg(all(feature = "tokio", not(target_family = "wasm")))]
mod tokio_support {
    use std::sync::OnceLock;

    use super::*;

    /// A lazily-created multi-thread tokio runtime shared by all futures.
    fn runtime() -> &'static tokio::runtime::Runtime {
        static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
        RT.get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("build tokio runtime")
        })
    }

    /// The future counterpart of [`spawn`]: drive `fut` on the tokio runtime, then
    /// deliver its output to `on_done` on the UI thread (via the same [`pump`]).
    /// Additive name because a closure and a future can't share one fn signature.
    pub fn spawn_future<T, Fut, D>(fut: Fut, on_done: D)
    where
        T: Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
        D: FnOnce(T) + 'static,
    {
        let slot: Arc<Mutex<Option<T>>> = Arc::new(Mutex::new(None));
        let write = slot.clone();
        let waker = current_waker();
        let wakes = waker.is_some();
        runtime().spawn(async move {
            let result = fut.await;
            if let Ok(mut s) = write.lock() {
                *s = Some(result);
            }
            if let Some(wake) = waker {
                wake();
            }
        });
        if !wakes {
            request_frame();
        }
        let mut on_done = Some(on_done);
        let poll: Poller = Box::new(move || match slot.lock().ok().and_then(|mut s| s.take()) {
            Some(v) => {
                if let Some(cb) = on_done.take() {
                    cb(v);
                }
                true
            }
            None => false,
        });
        PENDING.with(|p| p.borrow_mut().push(Pending { poll, wakes }));
    }

    /// The future counterpart of [`create_resource`]: `make` runs once on mount to
    /// build the future; the returned signal is `Loading` until it resolves.
    pub fn create_resource_future<T, Fut, MakeFut>(make: MakeFut) -> Signal<Resource<T>>
    where
        T: Send + Clone + 'static,
        Fut: Future<Output = T> + Send + 'static,
        MakeFut: FnOnce() -> Fut + 'static,
    {
        let state = create_signal(Resource::Loading);
        let cell = std::rc::Rc::new(RefCell::new(Some(make)));
        create_effect(move || {
            if let Some(mk) = cell.borrow_mut().take() {
                spawn_future(mk(), move |v| state.set(Resource::Ready(v)));
            }
        });
        state
    }
}

#[cfg(all(feature = "tokio", not(target_family = "wasm")))]
pub use tokio_support::{create_resource_future, spawn_future};

#[cfg(all(test, feature = "tokio", not(target_family = "wasm")))]
mod tokio_tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn ready_future_delivers_after_pump() {
        let got = Rc::new(Cell::new(0));
        let g = got.clone();
        spawn_future(async { 42 }, move |v| g.set(v));
        // The runtime resolves the ready future near-instantly; poll pump briefly.
        for _ in 0..2000 {
            pump();
            if got.get() != 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(got.get(), 42, "future output delivered on the UI thread");
    }
}

/// Drain finished background tasks and deliver their results (writing signals) on the
/// UI thread. Called once per frame by the shell. Returns whether the shell must keep
/// requesting frames to poll in-flight work: `true` only while a task that cannot wake
/// the shell (no [`Waker`] installed when it was spawned) is still running.
pub fn pump() -> bool {
    // Take the list so a task's `on_done` may itself `spawn` (re-entrant) without
    // aliasing the borrow; newly-spawned tasks land in the fresh list and are merged.
    let mut taken = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
    taken.retain_mut(|t| !(t.poll)());
    PENDING.with(|p| {
        let mut cur = p.borrow_mut();
        taken.append(&mut cur);
        *cur = taken;
        cur.iter().any(|t| !t.wakes)
    })
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    /// Work runs on the shared pool (no thread per task) and a delayed start waits.
    #[test]
    fn spawn_uses_the_pool_and_spawn_after_waits() {
        set_waker(None);
        let names = Rc::new(RefCell::new(Vec::new()));
        for _ in 0..3 {
            let n = names.clone();
            spawn(|| std::thread::current().name().map(String::from), move |v| n.borrow_mut().push(v));
            let t0 = std::time::Instant::now();
            while pump() && t0.elapsed().as_secs() < 5 {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        }
        assert!(names.borrow().iter().all(|n| n.as_deref() == Some("pebbles-worker")), "{names:?}");
        let at = Rc::new(Cell::new(None));
        let a = at.clone();
        let t0 = std::time::Instant::now();
        spawn_after(std::time::Duration::from_millis(60), || (), move |()| a.set(Some(t0.elapsed())));
        while at.get().is_none() && t0.elapsed().as_secs() < 5 {
            pump();
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(at.get().is_some_and(|d| d >= std::time::Duration::from_millis(60)), "{:?}", at.get());
    }

    /// M35: with a waker, in-flight work does not keep the shell drawing; the worker
    /// wakes it once, and the next pump delivers.
    #[test]
    fn a_woken_task_needs_no_polling_frames() {
        let woke = Arc::new(AtomicUsize::new(0));
        let w = woke.clone();
        set_waker(Some(Arc::new(move || {
            w.fetch_add(1, Ordering::SeqCst);
        })));
        let (go, gate) = mpsc::channel::<()>();
        let got = Rc::new(Cell::new(0));
        let g = got.clone();
        spawn(
            move || {
                gate.recv().ok();
                7
            },
            move |v| g.set(v),
        );
        assert!(!pump(), "a task that wakes the shell needs no polling frames");
        go.send(()).unwrap();
        for _ in 0..2000 {
            if woke.load(Ordering::SeqCst) == 1 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(woke.load(Ordering::SeqCst), 1, "the worker woke the shell once");
        assert!(!pump());
        assert_eq!(got.get(), 7, "the woken pump delivered the result");
        set_waker(None);
    }

    /// Without a waker (headless), in-flight work asks for polling frames until it lands.
    #[test]
    fn an_unwoken_task_keeps_polling() {
        set_waker(None);
        let (go, gate) = mpsc::channel::<()>();
        let got = Rc::new(Cell::new(0));
        let g = got.clone();
        spawn(
            move || {
                gate.recv().ok();
                3
            },
            move |v| g.set(v),
        );
        assert!(pump(), "no waker: keep drawing to poll");
        go.send(()).unwrap();
        for _ in 0..2000 {
            if !pump() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(got.get(), 3);
    }
}

/// The shared background workers behind [`spawn`] (native): a cached pool — an idle worker
/// takes the next job; when all are busy (a job may block, e.g. on a pipe) a new worker
/// starts; a worker idle for 30 s exits. Replaces a fresh OS thread per task (watchers
/// polling every 300 ms created ~3 threads a second for the app's lifetime).
#[cfg(not(target_family = "wasm"))]
mod pool {
    use std::collections::BinaryHeap;
    use std::sync::{Condvar, Mutex, OnceLock};
    use std::time::{Duration, Instant};

    pub(super) type Job = Box<dyn FnOnce() + Send>;

    struct Pool {
        queue: Mutex<(std::collections::VecDeque<Job>, usize)>, // (jobs, idle workers)
        ready: Condvar,
    }

    fn pool() -> &'static Pool {
        static P: OnceLock<Pool> = OnceLock::new();
        P.get_or_init(|| Pool { queue: Mutex::new((Default::default(), 0)), ready: Condvar::new() })
    }

    fn run_now(job: Job) {
        let p = pool();
        let mut q = p.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        q.0.push_back(job);
        if q.1 >= q.0.len() {
            p.ready.notify_one();
            return;
        }
        drop(q);
        let _ = std::thread::Builder::new().name("pebbles-worker".into()).spawn(worker);
    }

    fn worker() {
        let p = pool();
        let mut q = p.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            if let Some(job) = q.0.pop_front() {
                drop(q);
                job();
                q = p.queue.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                continue;
            }
            q.1 += 1;
            let (g, t) = p
                .ready
                .wait_timeout(q, Duration::from_secs(30))
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            q = g;
            q.1 -= 1;
            if t.timed_out() && q.0.is_empty() {
                return;
            }
        }
    }

    struct Delayed {
        at: Instant,
        seq: u64,
        job: Job,
    }
    impl PartialEq for Delayed {
        fn eq(&self, o: &Self) -> bool {
            (self.at, self.seq) == (o.at, o.seq)
        }
    }
    impl Eq for Delayed {}
    impl PartialOrd for Delayed {
        fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(o))
        }
    }
    impl Ord for Delayed {
        // Min-heap on (deadline, submission order).
        fn cmp(&self, o: &Self) -> std::cmp::Ordering {
            (o.at, o.seq).cmp(&(self.at, self.seq))
        }
    }

    struct Timer {
        heap: Mutex<(BinaryHeap<Delayed>, u64)>,
        changed: Condvar,
    }

    fn timer() -> &'static Timer {
        static T: OnceLock<Timer> = OnceLock::new();
        T.get_or_init(|| {
            let _ = std::thread::Builder::new().name("pebbles-timer".into()).spawn(timer_loop);
            Timer { heap: Mutex::new((BinaryHeap::new(), 0)), changed: Condvar::new() }
        })
    }

    fn timer_loop() {
        let t = timer();
        let mut h = t.heap.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            let now = Instant::now();
            while h.0.peek().is_some_and(|d| d.at <= now) {
                let d = h.0.pop().expect("peeked");
                run_now(d.job);
            }
            h = match h.0.peek().map(|d| d.at) {
                Some(at) => t.changed.wait_timeout(h, at - now).unwrap_or_else(std::sync::PoisonError::into_inner).0,
                None => t.changed.wait(h).unwrap_or_else(std::sync::PoisonError::into_inner),
            };
        }
    }

    /// Run `job` on a pool worker after `delay` (now when zero).
    pub(super) fn submit(delay: Duration, job: Job) {
        if delay.is_zero() {
            return run_now(job);
        }
        let t = timer();
        let mut h = t.heap.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        h.1 += 1;
        let seq = h.1;
        h.0.push(Delayed { at: Instant::now() + delay, seq, job });
        t.changed.notify_one();
    }
}
