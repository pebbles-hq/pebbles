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
new_key_type! {
    /// A derived (memo) node — lazy, three-color. Its value lives in a
    /// [`SignalSlot`] (so it reuses the read API + subscriber sets); the node
    /// carries the compute closure, its state, and the sources it read.
    struct MemoId;
}

/// The three-color state of a lazy memo (Reactively/Leptos/Svelte model):
/// `Clean` = value is current; `Dirty` = a source definitely changed, must
/// recompute; `Check` = a source *might* have changed — pull the sources on read
/// and recompute only if one actually did. `Ord` so `mark` never downgrades.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum NodeState {
    Clean = 0,
    Check = 1,
    Dirty = 2,
}

/// A component instance's identity across the runtime: which **window** (`Ui`) it
/// lives in, plus its element id. Each window has an independent element arena, so
/// element ids collide between windows — the window number disambiguates them. The
/// single-window case is simply window `0`.
pub(crate) type CompKey = (u32, ElementId);

/// Whatever is currently "reading" signals: a component instance, an effect, or a
/// memo recomputing (which tracks the sources it reads).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Observer {
    Component(CompKey),
    Effect(EffectId),
    Memo(MemoId),
}

struct SignalSlot {
    value: Box<dyn Any>,
    component_subs: HashSet<CompKey>,
    effect_subs: HashSet<EffectId>,
    /// Memos that read this signal (they re-subscribe when they recompute, so this
    /// is NOT drained on write — only the memo's own recompute manages it).
    memo_subs: HashSet<MemoId>,
    /// If this signal IS a memo's output, its backing memo — reading the signal
    /// pulls the memo first (lazy evaluation).
    memo: Option<MemoId>,
    /// Write-version this value last actually changed at — the Check-resolution
    /// test is `source.wv > memo.wv` (Svelte 5's `wv`; cheaper than a per-edge
    /// changed-handshake and trivially correct across diamonds).
    wv: u64,
}

impl SignalSlot {
    fn new(value: Box<dyn Any>) -> Self {
        SignalSlot {
            value,
            component_subs: HashSet::new(),
            effect_subs: HashSet::new(),
            memo_subs: HashSet::new(),
            memo: None,
            wv: 0,
        }
    }
}

/// A lazy derived node. The value lives in `output`'s [`SignalSlot`]; this carries
/// the compute closure, the current state, the sources it read (to walk on a
/// `Check` pull), and the write-version its value last changed at.
struct MemoNode {
    /// Recomputes under `observer = Memo(id)`: runs the user fn (rebuilding
    /// `sources` + the sources' `memo_subs`), compares to the stored value, writes
    /// it back only if changed, and returns whether it changed. Runs OUTSIDE any
    /// runtime borrow (it does its own reads), exactly like an effect.
    recompute: Rc<dyn Fn(MemoId) -> bool>,
    /// The signal holding this memo's value; its `wv` IS the memo's write-version
    /// (single source of truth), so a recompute bumps exactly one place.
    output: SignalId,
    state: NodeState,
    sources: HashSet<SignalId>,
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
    /// Lazy derived nodes (memos). Their values live in `signals`; these carry the
    /// compute + dependency state.
    memos: SlotMap<MemoId, MemoNode>,
    /// Global write-version, bumped on every value change (signal set or memo
    /// recompute-that-changed) and stamped onto the changed signal's `wv`.
    write_version: u64,
    /// Demanded memos (a component/effect reads them) that went non-clean and must
    /// be settled BEFORE render so the equality cut can decide whether to schedule
    /// their readers. Undemanded/intermediate memos stay lazy (never queued).
    pending_memos: Vec<MemoId>,
    pending_memos_set: HashSet<MemoId>,
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
    /// O(1) membership guard mirroring `pending_effects` (T1.4) — parity with the
    /// component set; replaces the linear `contains` on every schedule.
    pending_effects_set: HashSet<EffectId>,
    /// Reusable scratch buffers for the subscriber drain in `schedule_subscribers`
    /// (T1.5): a write moves the subscriber snapshot through these instead of
    /// allocating a fresh `Vec` each time. Kept at peak capacity, cleared not dropped.
    scratch_comps: Vec<CompKey>,
    scratch_effects: Vec<EffectId>,
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
            let id = rt.signals.insert(SignalSlot::new(Box::new(value)));
            rt.hooks.entry(owner).or_default().push(id);
            #[cfg(debug_assertions)]
            rt.hook_types.entry(owner).or_default().push(std::any::TypeId::of::<T>());
            Signal { id, _marker: PhantomData }
        } else {
            let id = rt.signals.insert(SignalSlot::new(Box::new(value)));
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
        let id = rt.signals.insert(SignalSlot::new(Box::new(value)));
        Signal { id, _marker: PhantomData }
    })
}

/// Free an app-scoped signal created with [`create_root_signal`]: its arena slot
/// (and any lingering subscriptions) are dropped. Call it only when the signal's
/// last reader is unmounting — reading a freed signal is a bug. Registry-keyed
/// primitives (e.g. `use_bounds`) use this to return to baseline on unmount
/// instead of leaking one root signal per remount.
pub fn dispose_root_signal<T: 'static + Clone>(sig: Signal<T>) {
    with_rt(|rt| dispose_signal(rt, sig.id));
}

/// Remove a signal slot and, if it backs a memo, the memo node too — detaching
/// that memo from every source it subscribed to and from the pending-settle queue.
fn dispose_signal(rt: &mut Runtime, sid: SignalId) {
    if let Some(mid) = rt.signals.get(sid).and_then(|s| s.memo) {
        if let Some(node) = rt.memos.remove(mid) {
            for src in node.sources {
                if let Some(s) = rt.signals.get_mut(src) {
                    s.memo_subs.remove(&mid);
                }
            }
        }
        if rt.pending_memos_set.remove(&mid) {
            rt.pending_memos.retain(|&m| m != mid);
        }
    }
    rt.signals.remove(sid);
}

impl<T: 'static + Clone> Signal<T> {
    /// Read the value, subscribing the current component/effect/memo to changes.
    pub fn get(&self) -> T {
        // If this signal is a memo's output, PULL it first (lazy evaluation) —
        // done outside the read borrow, since a recompute runs user code.
        self.pull_if_memo();
        with_rt(|rt| {
            assert!(
                rt.signals.contains_key(self.id),
                "signal read after dispose — a callback outlived its component; \
                 use try_peek() in timers/closures that can fire after unmount"
            );
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
                    Observer::Memo(mid) => {
                        // A recomputing memo tracks this signal as a source (both
                        // directions, so a write can find + a recompute can clear).
                        rt.signals[self.id].memo_subs.insert(mid);
                        if let Some(node) = rt.memos.get_mut(mid) {
                            node.sources.insert(self.id);
                        }
                    }
                }
            }
            rt.signals[self.id].value.downcast_ref::<T>().unwrap().clone()
        })
    }

    /// Read without subscribing (but still pulls a stale memo so the value is
    /// current).
    pub fn peek(&self) -> T {
        self.pull_if_memo();
        with_rt(|rt| {
            let slot = rt.signals.get(self.id).unwrap_or_else(|| {
                panic!(
                    "signal read after dispose — a callback outlived its component; \
                     use try_peek() in timers/closures that can fire after unmount"
                )
            });
            slot.value.downcast_ref::<T>().unwrap().clone()
        })
    }

    /// Read without subscribing; `None` once the owning component unmounted.
    /// THE safe read for timer callbacks and any closure that can outlive its
    /// component (mirrors how [`set`](Self::set)/[`update`](Self::update) no-op
    /// after dispose).
    pub fn try_peek(&self) -> Option<T> {
        self.pull_if_memo();
        with_rt(|rt| {
            rt.signals.get(self.id).map(|s| s.value.downcast_ref::<T>().unwrap().clone())
        })
    }

    /// If this signal backs a memo, bring it up to date (Reactively's
    /// `updateIfNecessary`) BEFORE any read borrow — a no-op for a plain signal.
    fn pull_if_memo(&self) {
        let memo = with_rt(|rt| rt.signals.get(self.id).and_then(|s| s.memo));
        if let Some(mid) = memo {
            update_if_necessary(mid);
        }
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
            if let Some(slot) = rt.signals.get_mut(self.id) {
                crate::reactive_stats::bump_write();
                write_value(slot, value);
                mark_written(rt, self.id);
            }
        });
    }

    /// Mutate the value in place and schedule dependents. The closure must not touch
    /// the runtime (don't read/write *this* signal inside it). A no-op if the signal
    /// was already disposed (mirrors [`set`](Self::set)), so a lingering handler on an
    /// unmounted component can never use-after-free.
    pub fn update(&self, f: impl FnOnce(&mut T)) {
        // Take the value box OUT — replaced by a zero-sized placeholder that never
        // allocates — run `f` OUTSIDE the runtime borrow (so it may still touch
        // OTHER signals, as before), then put the SAME box back. No clone of `T`,
        // no re-box. If `f` unmounts this signal's owner, the slot is gone on the
        // way back and we simply drop the box (the no-op-after-dispose contract).
        let taken: Option<Box<dyn Any>> = with_rt(|rt| {
            rt.signals.get_mut(self.id).map(|s| std::mem::replace(&mut s.value, Box::new(())))
        });
        let Some(mut boxed) = taken else { return };
        crate::reactive_stats::bump_write();
        f(boxed.downcast_mut::<T>().expect("Signal<T> slot always holds a T"));
        with_rt(|rt| {
            if let Some(s) = rt.signals.get_mut(self.id) {
                s.value = boxed;
                mark_written(rt, self.id);
            }
        });
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
            crate::reactive_stats::bump_write();
            write_value(&mut rt.signals[self.id], value);
            mark_written(rt, self.id);
            true
        })
    }
}

/// Overwrite a signal slot's value, reusing the existing `Box` when the concrete
/// type matches (T1.2) — which it always does for a well-typed `Signal<T>`, so no
/// heap allocation. The re-box branch is defensive and never fires in practice.
fn write_value<T: 'static>(slot: &mut SignalSlot, value: T) {
    if let Some(existing) = slot.value.downcast_mut::<T>() {
        *existing = value; // reuse the allocation
    } else {
        crate::reactive_stats::bump_box_alloc();
        slot.value = Box::new(value);
    }
}

/// A signal's value changed: stamp the write-version, then notify subscribers.
fn mark_written(rt: &mut Runtime, id: SignalId) {
    rt.write_version += 1;
    let wv = rt.write_version;
    rt.signals[id].wv = wv;
    schedule_subscribers(rt, id);
}

/// Notify every kind of subscriber of a changed signal: schedule its leaf readers
/// (components/effects) and mark its memo readers stale (they're pulled lazily).
fn schedule_subscribers(rt: &mut Runtime, id: SignalId) {
    schedule_leaf_subscribers(rt, id);
    // Memo subscribers: a DIRECT source changed, so they are definitely Dirty. The
    // mark recurses to *their* memo readers as Check and queues any demanded memo.
    // (Not drained — memos re-subscribe when they recompute, not per render.)
    let memo_subs: Vec<MemoId> = rt.signals[id].memo_subs.iter().copied().collect();
    for m in memo_subs {
        mark_memo(rt, m, NodeState::Dirty);
    }
    rt.frame_requested = true;
}

/// Schedule only the LEAF readers (components/effects) of a signal — the settle
/// phase uses this to wake the readers of a memo that actually changed, without
/// re-marking the memo graph. Drains the sub sets (leaves re-subscribe on render).
fn schedule_leaf_subscribers(rt: &mut Runtime, id: SignalId) {
    // Move the subscriber snapshot through reusable scratch buffers (T1.5) instead
    // of allocating a fresh `Vec` per write. `HashSet::drain` keeps the set's own
    // capacity, and the scratch buffers keep theirs — steady-state zero-alloc.
    let mut comps = std::mem::take(&mut rt.scratch_comps);
    let mut effs = std::mem::take(&mut rt.scratch_effects);
    let (cap_c, cap_e) = (comps.capacity(), effs.capacity());
    comps.extend(rt.signals[id].component_subs.drain());
    effs.extend(rt.signals[id].effect_subs.drain());
    if comps.capacity() > cap_c || effs.capacity() > cap_e {
        crate::reactive_stats::bump_vec_alloc();
    }
    for c in comps.drain(..) {
        crate::reactive_stats::bump_notify();
        if rt.pending_components_set.insert(c) {
            crate::reactive_stats::bump_component_schedule();
            rt.pending_components.push(c);
        }
    }
    for e in effs.drain(..) {
        crate::reactive_stats::bump_notify();
        if rt.pending_effects_set.insert(e) {
            rt.pending_effects.push(e);
        }
    }
    rt.scratch_comps = comps;
    rt.scratch_effects = effs;
}

/// Mark a memo non-clean and propagate (Reactively's `stale`): a direct source
/// change marks the memo `Dirty`, transitive changes mark `Check`. The
/// `if node.state >= state` guard makes the mark idempotent so it never revisits
/// a subgraph. NO computation happens here — this is pure flag-flipping. A memo
/// whose value has a leaf reader (a demanded memo) is queued for the settle phase,
/// which pulls it before render so the equality cut can decide whether to wake
/// that reader; undemanded/intermediate memos stay lazy (pulled on read).
fn mark_memo(rt: &mut Runtime, mid: MemoId, state: NodeState) {
    let Some(node) = rt.memos.get_mut(mid) else { return };
    if node.state >= state {
        return; // already at least this stale — its subscribers were already marked
    }
    node.state = state;
    let output = node.output;
    // Demanded (a component/effect reads it) → settle before render.
    let demanded = {
        let out = &rt.signals[output];
        !out.component_subs.is_empty() || !out.effect_subs.is_empty()
    };
    if demanded && rt.pending_memos_set.insert(mid) {
        rt.pending_memos.push(mid);
    }
    // Recurse to downstream memos as Check (they *might* change).
    let downstream: Vec<MemoId> = rt.signals[output].memo_subs.iter().copied().collect();
    for d in downstream {
        mark_memo(rt, d, NodeState::Check);
    }
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
    crate::reactive_stats::bump_effect_run();
    let prev = with_rt(|rt| {
        let p = rt.observer;
        rt.observer = Some(Observer::Effect(id));
        p
    });
    func();
    with_rt(|rt| rt.observer = prev);
}

/// A cached derived value — **lazy** (push-pull, three-color): a write to an input
/// marks the memo stale (a flag flip, no computation); the memo recomputes only
/// when something that is actually rendered pulls it. Because `T: PartialEq`, a
/// recompute that lands on the same value does NOT wake the memo's readers (the
/// equality-cut firewall), and a memo that nothing reads this frame does not
/// recompute at all. Create it once, at a stable position (app scope or the top of
/// a component, like any signal). Reads (`get`/`peek`) go through the returned
/// `Signal<T>` exactly as before — the laziness is invisible at the call site.
pub fn create_memo<T: 'static + Clone + PartialEq>(f: impl Fn() -> T + 'static) -> Signal<T> {
    // The output value lives in an ordinary signal (reusing the read API + the
    // subscriber sets). The initial value is computed UNTRACKED; the tracked
    // recompute below wires the sources. Position-stable inside a component: the
    // output signal persists by hook order, and the memo node is created only on
    // the first render (when the signal isn't yet linked to a memo).
    let signal = create_signal(untrack(&f));
    let already_linked = with_rt(|rt| rt.signals[signal.id].memo.is_some());
    if already_linked {
        return signal; // re-render: the memo node already exists
    }

    let output = signal.id;
    // The type-erased recompute: runs the user fn under `observer = Memo(id)`
    // (set by the caller), compares to the stored value, writes it back and bumps
    // the write-version only when it changed, and reports whether it changed.
    let recompute: Rc<dyn Fn(MemoId) -> bool> = Rc::new(move |_mid| {
        crate::reactive_stats::bump_memo_recompute();
        let value = f(); // reads its inputs (observer is Memo(mid)); done outside any borrow
        with_rt(|rt| {
            let Some(slot) = rt.signals.get_mut(output) else { return false };
            let changed = slot.value.downcast_ref::<T>().is_none_or(|old| old != &value);
            if changed {
                write_value(slot, value);
                rt.write_version += 1;
                let wv = rt.write_version;
                rt.signals[output].wv = wv;
            }
            changed
        })
    });

    let mid = with_rt(|rt| {
        let mid = rt.memos.insert(MemoNode {
            recompute,
            output,
            // Born Dirty with the initial value already stored: the first pull below
            // wires the sources (a tracked run) and settles to Clean without waking
            // anyone (the value equals the untracked initial).
            state: NodeState::Dirty,
            sources: HashSet::new(),
        });
        rt.signals[output].memo = Some(mid);
        mid
    });
    // Wire the source subscriptions now (a tracked recompute) so a later input
    // write can find this memo. The value is unchanged, so no one is scheduled.
    update_if_necessary(mid);
    signal
}

/// Bring a memo up to date if a source might have changed (Reactively's
/// `updateIfNecessary`). Runs OUTSIDE any runtime borrow (a recompute runs user
/// code). Clean → nothing; Dirty → recompute; Check → pull each source (recursing
/// into source memos), recompute only if a source's write-version moved past ours.
fn update_if_necessary(mid: MemoId) {
    let state = with_rt(|rt| rt.memos.get(mid).map(|n| n.state));
    match state {
        None | Some(NodeState::Clean) => {}
        Some(NodeState::Dirty) => {
            recompute_memo(mid);
        }
        Some(NodeState::Check) => {
            // Pull sources; a source whose value changed past our last-seen version
            // makes us Dirty. Early-out on the first such source (Reactively's break).
            // Our version IS our output signal's `wv`.
            let (sources, my_wv) = with_rt(|rt| {
                let node = &rt.memos[mid];
                let wv = rt.signals.get(node.output).map(|s| s.wv).unwrap_or(0);
                (node.sources.iter().copied().collect::<Vec<_>>(), wv)
            });
            let mut dirty = false;
            for src in sources {
                // If the source is itself a memo, settle it first (recursive pull).
                let inner = with_rt(|rt| rt.signals.get(src).and_then(|s| s.memo));
                if let Some(inner) = inner {
                    update_if_necessary(inner);
                }
                let src_wv = with_rt(|rt| rt.signals.get(src).map(|s| s.wv).unwrap_or(0));
                if src_wv > my_wv {
                    dirty = true;
                    break;
                }
            }
            if dirty {
                recompute_memo(mid);
            } else {
                with_rt(|rt| {
                    if let Some(n) = rt.memos.get_mut(mid) {
                        n.state = NodeState::Clean;
                    }
                });
            }
        }
    }
}

/// Recompute a memo: clear its old source edges, run the compute under
/// `observer = Memo(id)` (which rebuilds the edges), then mark Clean.
fn recompute_memo(mid: MemoId) {
    let recompute = with_rt(|rt| {
        let node = rt.memos.get(mid)?;
        let f = node.recompute.clone();
        // Clear old sources (both directions) so the tracked run rebuilds them —
        // an input no longer read must stop waking this memo.
        let old: Vec<SignalId> = node.sources.iter().copied().collect();
        for src in old {
            if let Some(s) = rt.signals.get_mut(src) {
                s.memo_subs.remove(&mid);
            }
        }
        rt.memos[mid].sources.clear();
        Some(f)
    });
    let Some(recompute) = recompute else { return };
    let prev = with_rt(|rt| {
        let p = rt.observer;
        rt.observer = Some(Observer::Memo(mid));
        p
    });
    recompute(mid); // reads rebuild sources + memo_subs; writes the value + bumps wv if changed
    with_rt(|rt| {
        rt.observer = prev;
        if let Some(n) = rt.memos.get_mut(mid) {
            n.state = NodeState::Clean;
        }
    });
}

/// Settle every demanded memo before render (the equality-cut phase): pull each,
/// and if its value actually changed, wake its leaf readers (components/effects).
/// Undemanded memos are never queued, so they never recompute here — that is the
/// lazy win. Loops because waking effects can, in turn, dirty more demanded memos.
fn settle_memos() {
    loop {
        let pending = with_rt(|rt| {
            rt.pending_memos_set.clear();
            std::mem::take(&mut rt.pending_memos)
        });
        if pending.is_empty() {
            break;
        }
        for mid in pending {
            // The memo's version is its output signal's `wv`. Snapshot it, pull, and
            // if it moved the value changed → wake exactly its leaf readers (its memo
            // readers were already marked Check during the write and pulled if demanded).
            let output = with_rt(|rt| rt.memos.get(mid).map(|n| n.output));
            let Some(output) = output else { continue };
            let before = with_rt(|rt| rt.signals.get(output).map(|s| s.wv).unwrap_or(0));
            update_if_necessary(mid);
            let after = with_rt(|rt| rt.signals.get(output).map(|s| s.wv).unwrap_or(0));
            if after != before {
                with_rt(|rt| schedule_leaf_subscribers(rt, output));
            }
        }
    }
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
                dispose_signal(rt, sid);
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

/// Settle the reactive graph before render: pull demanded memos (waking readers of
/// those that changed), then run scheduled effects — repeating because an effect
/// can dirty more memos. Called by `rebuild_if_dirty` each frame, before it renders
/// the scheduled components (which then read already-settled memos). Named
/// `flush_effects` for continuity with the shell's call site.
pub(crate) fn flush_effects() {
    loop {
        // Settle demanded memos first, so the equality cut has already decided which
        // components/effects to wake before we run/render anything.
        settle_memos();
        let pending = with_rt(|rt| {
            rt.pending_effects_set.clear(); // in lockstep with the Vec (T1.4)
            std::mem::take(&mut rt.pending_effects)
        });
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
