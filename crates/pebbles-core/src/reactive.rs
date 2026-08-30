//! SolidJS-style reactivity: [`create_signal`], [`create_memo`], [`create_effect`],
//! and [`create_store`]. The **same** `create_signal` primitive serves local state
//! (called inside a component) and global state (called at app scope) — that is the
//! Solid model.
//!
//! ## How it maps onto the engine
//! A thread-local reactive runtime tracks, per signal, which **components** (and
//! effects) read it. Writing a signal schedules those components for re-render; the
//! engine re-runs them and reconciles — so only the render objects that actually
//! changed are updated. Reads that happen while a component runs auto-subscribe
//! (no `setState`, no dependency arrays).
//!
//! Signals are `Copy` handles into the runtime, so they are trivially captured by
//! the plain-closure event handlers (`on_pressed(move || count.update(|c| *c += 1))`).

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashSet;
use std::marker::PhantomData;
use std::rc::Rc;

use slotmap::{SlotMap, new_key_type};

use crate::element::ElementId;

new_key_type! {
    /// Handle to a reactive value in the runtime.
    pub struct SignalId;
}
new_key_type! {
    struct EffectId;
}

/// A component instance's identity across the runtime: which **window** (`Ui`) it
/// lives in, plus its element id. Each window has an independent element arena, so
/// element ids collide between windows — the window number disambiguates them. The
/// single-window case is simply window `0`.
pub(crate) type CompKey = (u32, ElementId);

/// Whatever is currently "reading" signals: a component instance, or an effect.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Observer {
    Component(CompKey),
    Effect(EffectId),
}

struct SignalSlot {
    value: Box<dyn Any>,
    component_subs: HashSet<CompKey>,
    effect_subs: HashSet<EffectId>,
}

struct EffectSlot {
    func: Rc<dyn Fn()>,
}

#[derive(Default)]
struct Runtime {
    signals: SlotMap<SignalId, SignalSlot>,
    effects: SlotMap<EffectId, EffectSlot>,
    /// The observer currently running (for auto-subscription).
    observer: Option<Observer>,
    /// The component currently rendering (owns the local signals it creates).
    owner: Option<CompKey>,
    /// The window (`Ui`) currently rendering — folded into each `CompKey` so two
    /// windows' components never alias. Set by the `Ui` before it builds/reconciles.
    current_window: u32,
    /// Per-component ordered signal ids, for persisting local signals across
    /// re-renders (create signals at the top level of a component — the React rule).
    hooks: std::collections::HashMap<CompKey, Vec<SignalId>>,
    hook_cursor: usize,
    /// Per-component unmount callbacks (registry cleanup, etc). Re-registered each
    /// render (cleared in `begin_component`); run in `dispose_component`.
    cleanups: std::collections::HashMap<CompKey, Vec<Box<dyn FnOnce()>>>,
    /// A globally-unique u64 per component instance — the id the render-side
    /// registries (text-edit layout, scroll) key by, so they never collide across
    /// windows even though raw element ids do.
    instances: std::collections::HashMap<CompKey, u64>,
    next_instance: u64,
    /// Components scheduled to re-render.
    pending_components: Vec<CompKey>,
    /// Effects scheduled to re-run.
    pending_effects: Vec<EffectId>,
    /// Set when a write happened; the shell polls this to request a frame.
    frame_requested: bool,
}

thread_local! {
    static RT: RefCell<Runtime> = RefCell::new(Runtime::default());
}

fn with_rt<R>(f: impl FnOnce(&mut Runtime) -> R) -> R {
    RT.with(|rt| f(&mut rt.borrow_mut()))
}

// ---------------------------------------------------------------------------
// Signal
// ---------------------------------------------------------------------------

/// A reactive value. Cheap `Copy` handle; clone it freely into closures.
pub struct Signal<T> {
    id: SignalId,
    _marker: PhantomData<T>,
}

impl<T> Clone for Signal<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Signal<T> {}

impl<T> PartialEq for Signal<T> {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl<T> Eq for Signal<T> {}

/// Create a reactive value. Inside a component it is **local** state (persisted
/// across re-renders); at app scope it is **global** state shared by capture.
pub fn create_signal<T: 'static + Clone>(value: T) -> Signal<T> {
    with_rt(|rt| {
        // Local (owned by the current component): persist by creation order.
        if let Some(owner) = rt.owner {
            let index = rt.hook_cursor;
            rt.hook_cursor += 1;
            if let Some(existing) = rt.hooks.get(&owner).and_then(|v| v.get(index)).copied() {
                return Signal { id: existing, _marker: PhantomData };
            }
            let id = rt.signals.insert(SignalSlot {
                value: Box::new(value),
                component_subs: HashSet::new(),
                effect_subs: HashSet::new(),
            });
            rt.hooks.entry(owner).or_default().push(id);
            Signal { id, _marker: PhantomData }
        } else {
            let id = rt.signals.insert(SignalSlot {
                value: Box::new(value),
                component_subs: HashSet::new(),
                effect_subs: HashSet::new(),
            });
            Signal { id, _marker: PhantomData }
        }
    })
}

impl<T: 'static + Clone> Signal<T> {
    /// Read the value, subscribing the current component/effect to changes.
    pub fn get(&self) -> T {
        with_rt(|rt| {
            if let Some(observer) = rt.observer {
                let slot = &mut rt.signals[self.id];
                match observer {
                    Observer::Component(id) => {
                        slot.component_subs.insert(id);
                    }
                    Observer::Effect(id) => {
                        slot.effect_subs.insert(id);
                    }
                }
            }
            rt.signals[self.id].value.downcast_ref::<T>().unwrap().clone()
        })
    }

    /// Read without subscribing.
    pub fn peek(&self) -> T {
        with_rt(|rt| rt.signals[self.id].value.downcast_ref::<T>().unwrap().clone())
    }

    /// This signal's stable id as a `u64` — for keying per-signal registries (e.g. a
    /// scroll controller keys its viewport by its offset signal).
    pub fn raw_id(&self) -> u64 {
        use slotmap::Key;
        self.id.data().as_ffi()
    }

    /// Whether this signal still exists (false once its owning component unmounted).
    /// Long-lived holders (the animation driver) check this to drop stale references.
    pub fn alive(&self) -> bool {
        with_rt(|rt| rt.signals.contains_key(self.id))
    }

    /// Replace the value and schedule dependents. A no-op if the signal was already
    /// disposed (e.g. a lingering animation writing to an unmounted component).
    pub fn set(&self, value: T) {
        with_rt(|rt| {
            if rt.signals.contains_key(self.id) {
                rt.signals[self.id].value = Box::new(value);
                schedule_subscribers(rt, self.id);
            }
        });
    }

    /// Mutate the value in place and schedule dependents. The closure must not touch
    /// the runtime (don't read/write other signals inside it). A no-op if the signal
    /// was already disposed (mirrors [`set`](Self::set)), so a lingering handler on an
    /// unmounted component can never use-after-free.
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        if !self.alive() {
            return;
        }
        // Clone out → run f outside the borrow → set back (avoids re-entrancy).
        let mut value = self.peek();
        f(&mut value);
        self.set(value);
    }

    /// Read via a borrow, subscribing.
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let value = self.get();
        f(&value)
    }
}

fn schedule_subscribers(rt: &mut Runtime, id: SignalId) {
    let components: Vec<CompKey> = rt.signals[id].component_subs.drain().collect();
    let effects: Vec<EffectId> = rt.signals[id].effect_subs.drain().collect();
    for c in components {
        if !rt.pending_components.contains(&c) {
            rt.pending_components.push(c);
        }
    }
    for e in effects {
        if !rt.pending_effects.contains(&e) {
            rt.pending_effects.push(e);
        }
    }
    rt.frame_requested = true;
}

// ---------------------------------------------------------------------------
// Effects & memos
// ---------------------------------------------------------------------------

/// Run `f` now, and re-run it whenever a signal it read changes.
pub fn create_effect(f: impl Fn() + 'static) {
    let id = with_rt(|rt| rt.effects.insert(EffectSlot { func: Rc::new(f) }));
    run_effect(id);
}

fn run_effect(id: EffectId) {
    let func = with_rt(|rt| rt.effects.get(id).map(|e| e.func.clone()));
    let Some(func) = func else { return };
    let prev = with_rt(|rt| {
        let p = rt.observer;
        rt.observer = Some(Observer::Effect(id));
        p
    });
    func();
    with_rt(|rt| rt.observer = prev);
}

/// A cached derived value: recomputes only when its inputs change.
pub fn create_memo<T: 'static + Clone>(f: impl Fn() -> T + 'static) -> Signal<T> {
    let signal = create_signal(f());
    create_effect(move || {
        let value = f();
        signal.set(value);
    });
    signal
}

// ---------------------------------------------------------------------------
// Store (nested global state, Solid `createStore` flavor)
// ---------------------------------------------------------------------------

/// A global store: a single signal holding a `Clone` state value, read reactively
/// and updated with `set`/`update`. For a Redux flavor, pair it with your own
/// action enum + reducer inside `update`.
#[derive(Clone, Copy)]
pub struct Store<S: 'static + Clone> {
    signal: Signal<S>,
}

/// Create a global [`Store`].
pub fn create_store<S: 'static + Clone>(initial: S) -> Store<S> {
    Store { signal: create_signal(initial) }
}

impl<S: 'static + Clone> Store<S> {
    /// Read the whole state (subscribing).
    pub fn get(&self) -> S {
        self.signal.get()
    }
    /// Select a slice of the state (subscribing).
    pub fn select<R>(&self, f: impl FnOnce(&S) -> R) -> R {
        self.signal.with(f)
    }
    /// Replace the state.
    pub fn set(&self, state: S) {
        self.signal.set(state);
    }
    /// Mutate the state in place (a reducer step).
    pub fn update(&self, f: impl FnOnce(&mut S)) {
        self.signal.update(f);
    }
}

// ---------------------------------------------------------------------------
// Engine hooks (used by the element tree + shell)
// ---------------------------------------------------------------------------

/// Opaque save-state returned by [`begin_component`].
pub(crate) struct ComponentGuard {
    owner: Option<CompKey>,
    observer: Option<Observer>,
    cursor: usize,
}

/// Begin rendering component `id` (in the current window): it becomes the signal
/// owner + read observer, and its local-signal cursor resets. Returns a guard to
/// restore afterwards.
pub(crate) fn begin_component(id: ElementId) -> ComponentGuard {
    with_rt(|rt| {
        let key = (rt.current_window, id);
        let guard = ComponentGuard { owner: rt.owner, observer: rt.observer, cursor: rt.hook_cursor };
        rt.owner = Some(key);
        rt.observer = Some(Observer::Component(key));
        rt.hook_cursor = 0;
        // Assign a stable, globally-unique instance id on first render.
        if !rt.instances.contains_key(&key) {
            rt.next_instance += 1;
            let iid = rt.next_instance;
            rt.instances.insert(key, iid);
        }
        // Cleanups are re-registered fresh each render (they only run on unmount).
        rt.cleanups.remove(&key);
        // Clear this component's old subscriptions so tracking is fresh each run.
        for slot in rt.signals.values_mut() {
            slot.component_subs.remove(&key);
        }
        guard
    })
}

/// Restore the observer/owner state saved by [`begin_component`].
pub(crate) fn end_component(guard: ComponentGuard) {
    with_rt(|rt| {
        rt.owner = guard.owner;
        rt.observer = guard.observer;
        rt.hook_cursor = guard.cursor;
    });
}

/// Free a component's local signals when it unmounts, after running its cleanups.
pub(crate) fn dispose_component(id: ElementId) {
    let key = with_rt(|rt| (rt.current_window, id));
    // Run unmount callbacks first (outside the borrow, so they may touch signals).
    let cleanups = with_rt(|rt| rt.cleanups.remove(&key).unwrap_or_default());
    for f in cleanups {
        f();
    }
    with_rt(|rt| {
        if let Some(ids) = rt.hooks.remove(&key) {
            for sid in ids {
                rt.signals.remove(sid);
            }
        }
        for slot in rt.signals.values_mut() {
            slot.component_subs.remove(&key);
        }
        rt.instances.remove(&key);
    });
}

/// Register a callback to run when the current component unmounts (React's
/// cleanup / Solid's `onCleanup`). Call it at the top level of a component; it is
/// re-registered each render and fires exactly once, on dispose.
pub fn create_cleanup(f: impl FnOnce() + 'static) {
    with_rt(|rt| {
        if let Some(owner) = rt.owner {
            rt.cleanups.entry(owner).or_default().push(Box::new(f));
        }
    });
}

/// Drain the components of `window` scheduled to re-render (each `Ui` drains only
/// its own, leaving other windows' pending work untouched).
pub(crate) fn take_pending_components(window: u32) -> Vec<ElementId> {
    with_rt(|rt| {
        let mut mine = Vec::new();
        rt.pending_components.retain(|&(w, e)| {
            if w == window {
                mine.push(e);
                false
            } else {
                true
            }
        });
        mine
    })
}

/// Run any scheduled effects (called before re-rendering components).
pub(crate) fn flush_effects() {
    loop {
        let pending = with_rt(|rt| std::mem::take(&mut rt.pending_effects));
        if pending.is_empty() {
            break;
        }
        for id in pending {
            run_effect(id);
        }
    }
}

/// Whether a write requested a new frame; clears the flag.
pub fn frame_requested() -> bool {
    with_rt(|rt| std::mem::replace(&mut rt.frame_requested, false))
}

/// The component element currently rendering, if any. Used to give a focus node a
/// stable identity (the component's element id).
pub fn current_owner() -> Option<ElementId> {
    with_rt(|rt| rt.owner.map(|(_, e)| e))
}

/// The window (`Ui`) currently rendering — `0` is the main window.
pub fn current_window() -> u32 {
    with_rt(|rt| rt.current_window)
}

/// Set the window whose `Ui` is about to build/reconcile. Called by each `Ui`.
pub(crate) fn set_current_window(window: u32) {
    with_rt(|rt| rt.current_window = window);
}

/// The current component's globally-unique instance id (matches render-tree source
/// ids), for keying per-component registries (scroll, text-edit, …). Unique across
/// windows, unlike the raw element id.
pub fn owner_id() -> Option<u64> {
    with_rt(|rt| rt.owner.and_then(|key| rt.instances.get(&key).copied()))
}

/// The globally-unique instance id for `(window, element)`, assigning one if the
/// component hasn't rendered yet. Lets a focus node recover its render-registry key.
pub fn instance_id(window: u32, element: ElementId) -> u64 {
    with_rt(|rt| {
        let key = (window, element);
        if let Some(id) = rt.instances.get(&key) {
            *id
        } else {
            rt.next_instance += 1;
            let id = rt.next_instance;
            rt.instances.insert(key, id);
            id
        }
    })
}
