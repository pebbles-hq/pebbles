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

/// One entry on the render-time context stack — a value provided by a component
/// that stays visible while ITS subtree reconciles, then is popped when the
/// component's render completes. The basis for scoped theme overrides and focus
/// scopes (see [`provide_context`] / [`consume_context`]).
struct ContextEntry {
    owner: CompKey,
    value: Rc<dyn Any>,
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
    /// Render-time context stack: values a component provides to its own subtree
    /// while it renders. Popped in `end_component` once the subtree has reconciled.
    contexts: Vec<ContextEntry>,
    /// The window (`Ui`) currently rendering — folded into each `CompKey` so two
    /// windows' components never alias. Set by the `Ui` before it builds/reconciles.
    current_window: u32,
    /// Per-component ordered signal ids, for persisting local signals across
    /// re-renders (create signals at the top level of a component — the React rule).
    hooks: std::collections::HashMap<CompKey, Vec<SignalId>>,
    /// Parallel to `hooks`: the `TypeId` created at each position, for the debug-only
    /// hooks-order guardrail (empty in release).
    hook_types: std::collections::HashMap<CompKey, Vec<std::any::TypeId>>,
    hook_cursor: usize,
    /// Per-component unmount callbacks (registry cleanup, etc). Re-registered each
    /// render (cleared in `begin_component`); run in `dispose_component`.
    cleanups: std::collections::HashMap<CompKey, Vec<Box<dyn FnOnce()>>>,
    /// A globally-unique u64 per component instance — the id the render-side
    /// registries (text-edit layout, scroll) key by, so they never collide across
    /// windows even though raw element ids do.
    instances: std::collections::HashMap<CompKey, u64>,
    next_instance: u64,
    /// Reverse index: which signals each component is subscribed to. Lets
    /// `begin_component`/`dispose_component` clear a component's subscriptions in
    /// O(its own subs) instead of O(all signals in the app).
    subs_of: std::collections::HashMap<CompKey, HashSet<SignalId>>,
    /// Per-component ordered effect ids — the effect equivalent of `hooks`. An effect
    /// created in a component body is created ONCE (first render at its position) and
    /// persists across re-renders (it re-runs itself when its signal deps change, not
    /// on every owner re-render), then is disposed with the component. Without this an
    /// effect was recreated every render — leaking a slot per render and, for effects
    /// that write a signal the component reads (create_resource / ImageView), spinning
    /// forever (set → re-render → new effect → new fetch → set → …).
    effect_hooks: std::collections::HashMap<CompKey, Vec<EffectId>>,
    /// Cursor into the current component's `effect_hooks`, reset each render (parallel
    /// to `hook_cursor` for signals).
    effect_cursor: usize,
    /// Components scheduled to re-render (drain order).
    pending_components: Vec<CompKey>,
    /// O(1) membership guard mirroring `pending_components` (E1) — replaces the linear
    /// `contains` on every schedule; kept in lockstep with the Vec.
    pending_components_set: HashSet<CompKey>,
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
                // Hooks-rule guardrail (debug only): the signal reused at this position
                // must have the SAME type as last render. A different type here means
                // create_signal was called conditionally / in a different order, which
                // silently aliases the wrong slot. (Same-type positional reuse — incl.
                // a conditionally-created *trailing* signal — is safe and never trips.)
                #[cfg(debug_assertions)]
                {
                    let stored = rt.hook_types.get(&owner).and_then(|v| v.get(index)).copied();
                    debug_assert_eq!(
                        stored,
                        Some(std::any::TypeId::of::<T>()),
                        "Pebbles hooks rule: create_signal at position {index} of component \
                         {owner:?} changed type between renders. Never create signals \
                         conditionally or in a variable order — create them unconditionally at \
                         the top of the component."
                    );
                }
                return Signal { id: existing, _marker: PhantomData };
            }
            let id = rt.signals.insert(SignalSlot {
                value: Box::new(value),
                component_subs: HashSet::new(),
                effect_subs: HashSet::new(),
            });
            rt.hooks.entry(owner).or_default().push(id);
            #[cfg(debug_assertions)]
            rt.hook_types.entry(owner).or_default().push(std::any::TypeId::of::<T>());
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

/// Create an **app-owned** signal even when called from inside a component render.
///
/// Unlike [`create_signal`] — which, inside a component, creates a *local* signal
/// that counts against the hooks order and is disposed when that component unmounts —
/// this always creates a global signal owned by the app root. Use it to lazily
/// initialize global state (theme, overlay host, focus) on first touch, so a stray
/// early `theme()`/`show_overlay()` call from within a render can't accidentally bind
/// the global to a component (the hooks-order footgun). Never counts against hooks.
pub fn create_root_signal<T: 'static + Clone>(value: T) -> Signal<T> {
    with_rt(|rt| {
        let id = rt.signals.insert(SignalSlot {
            value: Box::new(value),
            component_subs: HashSet::new(),
            effect_subs: HashSet::new(),
        });
        Signal { id, _marker: PhantomData }
    })
}

/// Free an app-scoped signal created with [`create_root_signal`]: its arena slot
/// (and any lingering subscriptions) are dropped. Call it only when the signal's
/// last reader is unmounting — reading a freed signal is a bug. Registry-keyed
/// primitives (e.g. `use_bounds`) use this to return to baseline on unmount
/// instead of leaking one root signal per remount.
pub fn dispose_root_signal<T: 'static + Clone>(sig: Signal<T>) {
    with_rt(|rt| {
        rt.signals.remove(sig.id);
    });
}

impl<T: 'static + Clone> Signal<T> {
    /// Read the value, subscribing the current component/effect to changes.
    pub fn get(&self) -> T {
        with_rt(|rt| {
            if let Some(observer) = rt.observer {
                match observer {
                    Observer::Component(key) => {
                        rt.signals[self.id].component_subs.insert(key);
                        // Record the reverse edge so we can clear this component's
                        // subscriptions later without walking every signal.
                        rt.subs_of.entry(key).or_default().insert(self.id);
                    }
                    Observer::Effect(id) => {
                        rt.signals[self.id].effect_subs.insert(id);
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

impl<T: 'static + Clone + PartialEq> Signal<T> {
    /// Replace the value, but only wake dependents if it actually changed. Returns
    /// whether it changed. This is how [`create_memo`] avoids re-rendering downstream
    /// on a recompute that lands on the same value; use it directly to swallow no-op
    /// writes (e.g. a slider snapping to a value it already held).
    pub fn set_if_changed(&self, value: T) -> bool {
        with_rt(|rt| {
            if !rt.signals.contains_key(self.id) {
                return false;
            }
            if rt.signals[self.id].value.downcast_ref::<T>().unwrap() == &value {
                return false; // unchanged — no write, no reschedule
            }
            rt.signals[self.id].value = Box::new(value);
            schedule_subscribers(rt, self.id);
            true
        })
    }
}

fn schedule_subscribers(rt: &mut Runtime, id: SignalId) {
    let components: Vec<CompKey> = rt.signals[id].component_subs.drain().collect();
    let effects: Vec<EffectId> = rt.signals[id].effect_subs.drain().collect();
    for c in components {
        // O(1) dedup (E1): the set insert reports novelty; the Vec keeps drain order.
        if rt.pending_components_set.insert(c) {
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
///
/// **Inside a component** the effect is *position-stable*, exactly like
/// [`create_signal`]: it is created once (the first render that reaches this call) and
/// persists across re-renders — it re-runs on its own when a signal it read changes,
/// **not** on every re-render of the owning component — and is disposed when the
/// component unmounts. (Recreating it per render would leak an effect slot each render,
/// and an effect that writes a signal its component reads — `create_resource`,
/// `ImageView` — would spin forever.) Create effects at the top level of a component,
/// unconditionally, like any hook. **At app scope** (no owning component) the effect
/// lives for the process, as before.
pub fn create_effect(f: impl Fn() + 'static) {
    let owner = with_rt(|rt| rt.owner);
    let Some(key) = owner else {
        // App scope (e.g. `Channel::on`): untracked, lives for the app.
        let id = with_rt(|rt| rt.effects.insert(EffectSlot { func: Rc::new(f) }));
        run_effect(id);
        return;
    };
    // Position-stable within the component: reuse the effect created at this position
    // on a prior render (it's already live + reactive — do nothing), else create it.
    let index = with_rt(|rt| {
        let i = rt.effect_cursor;
        rt.effect_cursor += 1;
        i
    });
    let existing = with_rt(|rt| rt.effect_hooks.get(&key).and_then(|v| v.get(index)).copied());
    if existing.is_some() {
        return;
    }
    let id = with_rt(|rt| {
        let id = rt.effects.insert(EffectSlot { func: Rc::new(f) });
        rt.effect_hooks.entry(key).or_default().push(id);
        id
    });
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

/// A cached derived value: recomputes when its inputs change, and — because `T:
/// PartialEq` — only wakes *its own* dependents when the recomputed value actually
/// differs (Solid's memo dedup). A memo over a coarse projection (say `count % 2`)
/// therefore re-renders downstream only when the projection flips, not on every input
/// change. Create it once, at a stable position (app scope or the top of a component),
/// like any signal.
pub fn create_memo<T: 'static + Clone + PartialEq>(f: impl Fn() -> T + 'static) -> Signal<T> {
    // The initial compute runs UNTRACKED: the memo's effect (below) owns the
    // input subscriptions. Computing it under the calling component's observer
    // would subscribe the component to the memo's raw inputs — every input
    // write would then re-render it, defeating the memo's dedup entirely.
    let signal = create_signal(untrack(&f));
    create_effect(move || {
        let value = f();
        signal.set_if_changed(value);
    });
    signal
}

/// Run `f` with dependency tracking suspended: signal reads inside do NOT
/// subscribe the current component/effect (Solid's `untrack`). Use it to peek
/// at reactive state from inside a render without re-rendering on its changes.
pub fn untrack<T>(f: impl FnOnce() -> T) -> T {
    let prev = with_rt(|rt| {
        let p = rt.observer;
        rt.observer = None;
        p
    });
    let out = f();
    with_rt(|rt| rt.observer = prev);
    out
}

// ---------------------------------------------------------------------------
// Store (nested global state, Solid `createStore` flavor)
// ---------------------------------------------------------------------------

/// A global store: a single signal holding a `Clone` state value, read reactively
/// and updated with `set`/`update`. For a Redux flavor, pair it with your own
/// action enum + reducer inside `update`.
///
/// **Granularity — read this.** A `Store` is deliberately *coarse*: it is one signal,
/// so **any** `update` wakes **every** component that read the store, even ones that
/// only used an untouched field. That's the simple, predictable model. When you want a
/// field to re-render its readers *independently*, reach for one of the two blessed
/// finer-grained patterns instead of a big store:
///
/// 1. **Many small signals** (the default) — hold each independent piece of state in
///    its own `create_signal`. Writing one wakes only its readers. This is the Solid
///    way and what most app state should be.
/// 2. **A deduped memo slice** — derive a field with [`create_memo`], which only wakes
///    downstream when that slice's value actually changes:
///    ```ignore
///    let name = create_memo(move || store.select(|s| s.user.name.clone()));
///    // reading `name` re-renders only when `user.name` changes, not on any store write
///    ```
///    Create the memo once (app scope or a stable position), like any signal.
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
    /// A deduped selector (E4): returns a `Signal<R>` that recomputes `f` when the
    /// store changes but only propagates (re-renders downstream) when the selected
    /// slice actually changes (`PartialEq`). Sugar for
    /// `create_memo(move || store.select(&f))` — call it at the top level of a
    /// component, like any hook.
    ///
    /// ```ignore
    /// let name = store.select_memo(|s| s.user.name.clone()); // re-renders only when the name changes
    /// ```
    pub fn select_memo<R: 'static + Clone + PartialEq>(
        &self,
        f: impl Fn(&S) -> R + 'static,
    ) -> Signal<R> {
        let signal = self.signal;
        create_memo(move || signal.with(&f))
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
    /// This component's own key — so [`end_component`] can pop exactly the
    /// contexts this render provided (after its subtree has reconciled).
    key: CompKey,
    owner: Option<CompKey>,
    observer: Option<Observer>,
    cursor: usize,
    effect_cursor: usize,
}

/// Begin rendering component `id` (in the current window): it becomes the signal
/// owner + read observer, and its local-signal cursor resets. Returns a guard to
/// restore afterwards. The guard must stay in scope until the component's child
/// widget has been reconciled, so render-time contexts (theme overrides, focus
/// scopes) remain visible to the whole subtree.
pub(crate) fn begin_component(id: ElementId) -> ComponentGuard {
    with_rt(|rt| {
        let key = (rt.current_window, id);
        let guard = ComponentGuard {
            key,
            owner: rt.owner,
            observer: rt.observer,
            cursor: rt.hook_cursor,
            effect_cursor: rt.effect_cursor,
        };
        rt.owner = Some(key);
        rt.observer = Some(Observer::Component(key));
        rt.hook_cursor = 0;
        rt.effect_cursor = 0;
        // Assign a stable, globally-unique instance id on first render.
        if !rt.instances.contains_key(&key) {
            rt.next_instance += 1;
            let iid = rt.next_instance;
            rt.instances.insert(key, iid);
        }
        // Cleanups are re-registered fresh each render (they only run on unmount).
        rt.cleanups.remove(&key);
        // Clear this component's old subscriptions so tracking is fresh each run —
        // touching only the signals it actually read (via the reverse index).
        if let Some(sids) = rt.subs_of.remove(&key) {
            for sid in sids {
                if let Some(slot) = rt.signals.get_mut(sid) {
                    slot.component_subs.remove(&key);
                }
            }
        }
        guard
    })
}

/// Restore the observer/owner state saved by [`begin_component`], and pop the
/// render-time contexts this component provided — its subtree has reconciled by
/// now (the reconciler keeps the guard alive across the child update), so the
/// contexts are no longer visible.
///
/// Note on the "hooks rule": local signals persist by creation *order* (index), so a
/// signal created conditionally at a *stable trailing position* is safe (its slot is
/// never reused for something else). The genuinely unsafe case is an *order shift* —
/// index N mapping to a different logical signal between renders. A precise debug lint
/// for that (per-index identity tracking) is future work; a naive count check would
/// false-positive on the safe conditional-trailing pattern used across the catalog.
pub(crate) fn end_component(guard: ComponentGuard) {
    with_rt(|rt| {
        // Entries provided during this render sit on top (any descendant providers
        // already popped their own) — drop everything this component owns.
        while rt.contexts.last().is_some_and(|e| e.owner == guard.key) {
            rt.contexts.pop();
        }
        rt.owner = guard.owner;
        rt.observer = guard.observer;
        rt.hook_cursor = guard.cursor;
        rt.effect_cursor = guard.effect_cursor;
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
        rt.hook_types.remove(&key);
        // Free this component's effects (the effect equivalent of freeing its signals
        // above). A removed effect id may still linger in some surviving signal's
        // `effect_subs`, but that self-cleans: the next write to that signal drains the
        // id into `pending_effects`, and `run_effect` finds no slot and returns — so no
        // reverse index (and no O(all-signals) scan) is needed here.
        if let Some(eids) = rt.effect_hooks.remove(&key) {
            for eid in eids {
                rt.effects.remove(eid);
            }
        }
        // Drop this component's subscriptions via the reverse index (O(its own subs)).
        if let Some(sids) = rt.subs_of.remove(&key) {
            for sid in sids {
                if let Some(slot) = rt.signals.get_mut(sid) {
                    slot.component_subs.remove(&key);
                }
            }
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

// ---------------------------------------------------------------------------
// Debug-only census (lifecycle soak tripwire — see performance-standards.md E6c)
// ---------------------------------------------------------------------------

/// Number of live signals (debug-only).
#[cfg(debug_assertions)]
pub fn census_signals() -> usize {
    with_rt(|rt| rt.signals.len())
}

/// Number of active component→signal subscription edges (debug-only).
#[cfg(debug_assertions)]
pub fn census_subscriptions() -> usize {
    with_rt(|rt| rt.subs_of.values().map(|s| s.len()).sum())
}

/// Number of pending (registered, not-yet-run) unmount cleanups (debug-only).
#[cfg(debug_assertions)]
pub fn census_cleanups() -> usize {
    with_rt(|rt| rt.cleanups.values().map(|v| v.len()).sum())
}

/// Number of components + effects scheduled to re-run (debug-only).
#[cfg(debug_assertions)]
pub fn census_pending() -> usize {
    with_rt(|rt| rt.pending_components.len() + rt.pending_effects.len())
}

/// Provide a value to the current component's **whole subtree** while it renders.
/// Call at the top of a component's render function: every component rendered
/// inside its returned widget (however deeply nested) can read the value back with
/// [`consume_context`] until this component's render completes. Inner providers
/// shadow outer ones of the same type (most-recently-provided wins). At app scope
/// (no owning component) the call is a no-op.
pub fn provide_context<T: 'static>(value: T) {
    with_rt(|rt| {
        if let Some(owner) = rt.owner {
            rt.contexts.push(ContextEntry { owner, value: Rc::new(value) });
        }
    });
}

/// Read the innermost value of type `T` provided by an enclosing component (the
/// render-time equivalent of React context / Flutter's `Theme.of`). Returns `None`
/// outside a component subtree that provides `T`.
pub fn consume_context<T: 'static + Clone>() -> Option<T> {
    with_rt(|rt| {
        rt.contexts
            .iter()
            .rev()
            .find_map(|e| e.value.downcast_ref::<T>().cloned())
    })
}

/// Drain the components of `window` scheduled to re-render (each `Ui` drains only
/// its own, leaving other windows' pending work untouched).
pub(crate) fn take_pending_components(window: u32) -> Vec<ElementId> {
    with_rt(|rt| {
        // Take the Vec out first so the loop can also update the membership set
        // (E1) without a second borrow of `rt`.
        let all = std::mem::take(&mut rt.pending_components);
        let mut mine = Vec::new();
        let mut keep = Vec::with_capacity(all.len());
        for (w, e) in all {
            if w == window {
                rt.pending_components_set.remove(&(w, e));
                mine.push(e);
            } else {
                keep.push((w, e));
            }
        }
        rt.pending_components = keep;
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

/// Request that the shell draw another frame. Called on the UI thread by machinery
/// that changes what should be on screen without writing a signal (e.g. registering a
/// background [`task`](crate::task) whose result will land on a later frame).
pub fn request_frame() {
    with_rt(|rt| rt.frame_requested = true);
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
