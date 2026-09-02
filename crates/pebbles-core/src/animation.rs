//! A tiny animation driver — the ticker behind smooth transitions.
//!
//! A **track** interpolates one `Signal<f64>` from a start value to a target over a
//! duration, with an ease-out curve. The shell calls [`tick`] once per frame with a
//! monotonic timestamp; each active track advances and writes its signal (which
//! re-renders whatever reads it). While [`active`] is true the shell keeps
//! requesting frames, so animations run without the app polling.
//!
//! The ergonomic entry point is [`animated`]: call it in a component with a target
//! value and it returns the current interpolated value, starting a new transition
//! only when the target actually changes. Perfect for "animate the switch thumb
//! whenever the boolean flips":
//!
//! ```ignore
//! let pos = animated(if on { 1.0 } else { 0.0 }, 0.16); // 0.0..=1.0, smooth
//! ```

use std::cell::RefCell;
use std::rc::Rc;

use crate::reactive::{Signal, create_signal};

#[derive(Clone, Copy)]
struct Track {
    value: Signal<f64>,
    from: f64,
    to: f64,
    /// Filled on the first tick that sees this track (so timing starts at paint).
    start: Option<f64>,
    duration: f64,
    curve: Curve,
}

/// A continuous, repeating driver (spinners, indeterminate progress). Its signal
/// cycles `0.0..1.0` every `period` seconds until the owning component unmounts.
struct Loop {
    value: Signal<f64>,
    period: f64,
}

/// A one-shot delay: fires `action` once, `delay` seconds after it is registered.
struct Timeout {
    /// Absolute fire time; filled on the first tick that sees it (so timing starts at
    /// paint, like a track's `start`).
    at: Option<f64>,
    delay: f64,
    action: Rc<dyn Fn()>,
    /// Liveness handle — for the [`create_timeout`] hook, a component-owned signal; when
    /// its component unmounts the signal dies and the timeout is dropped. `None` for a
    /// keyed [`set_timeout`] whose lifecycle the caller owns (fire or `clear_timeout`).
    guard: Option<Signal<()>>,
}

thread_local! {
    static TRACKS: RefCell<Vec<Track>> = const { RefCell::new(Vec::new()) };
    static LOOPS: RefCell<std::collections::HashMap<u64, Loop>> =
        RefCell::new(std::collections::HashMap::new());
    static TIMEOUTS: RefCell<std::collections::HashMap<u64, Timeout>> =
        RefCell::new(std::collections::HashMap::new());
}

/// Ease-out cubic — fast start, gentle settle. The default UI curve.
fn ease_out_cubic(t: f64) -> f64 {
    let f = 1.0 - t;
    1.0 - f * f * f
}

/// An easing curve mapping linear progress `0..=1` onto eased progress.
/// Flutter's `Curves` vocabulary, the subset every UI actually needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Curve {
    /// Slow at both ends, fast in the middle (the default when unspecified).
    #[default]
    EaseInOut,
    /// Constant speed.
    Linear,
    /// Slow start, accelerating.
    EaseIn,
    /// Fast start, decelerating.
    EaseOut,
    /// Ease-out with a cubic shape (the house default for tweens).
    EaseOutCubic,
}

impl Curve {
    /// Map linear progress `t` (already clamped to `0..=1`) through the curve.
    pub fn apply(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Curve::Linear => t,
            Curve::EaseIn => t * t * t,
            Curve::EaseOut => ease_out_cubic(t),
            Curve::EaseInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Curve::EaseOutCubic => ease_out_cubic(t),
        }
    }
}

/// Animate `value` from its current reading to `to` over `duration` seconds. A new
/// call to the same signal replaces any in-flight animation (smooth reversal). A
/// zero-distance change snaps immediately.
pub fn animate_to(value: Signal<f64>, to: f64, duration: f64) {
    animate_to_with(value, to, duration, Curve::EaseOutCubic);
}

/// Like [`animate_to`], with an explicit easing [`Curve`].
pub fn animate_to_with(value: Signal<f64>, to: f64, duration: f64, curve: Curve) {
    let from = value.peek();
    TRACKS.with(|t| {
        let mut t = t.borrow_mut();
        t.retain(|tr| tr.value != value);
        if duration <= 0.0 || (from - to).abs() <= 1e-4 {
            value.set(to);
        } else {
            t.push(Track { value, from, to, start: None, duration, curve });
        }
    });
}

/// Whether any animation (tween, loop, or pending timeout) is currently running (the
/// shell polls this to decide whether to schedule another frame).
pub fn active() -> bool {
    TRACKS.with(|t| !t.borrow().is_empty())
        || LOOPS.with(|l| !l.borrow().is_empty())
        || TIMEOUTS.with(|t| !t.borrow().is_empty())
}

/// Number of live loops (debug-only).
#[cfg(debug_assertions)]
pub fn census_loops() -> usize {
    LOOPS.with(|l| l.borrow().len())
}

/// Number of pending timeouts (debug-only).
#[cfg(debug_assertions)]
pub fn census_timeouts() -> usize {
    TIMEOUTS.with(|t| t.borrow().len())
}

/// A component hook: run `f` **once**, `secs` seconds from now. The delay is anchored
/// at the next frame (paint), ticked by the driver, and fires exactly once — then it
/// is removed. If the owning component unmounts first, it never fires. The basis for
/// tooltip show-delays and toast auto-dismiss. Call it at the top level of a component.
pub fn create_timeout(secs: f64, f: impl Fn() + 'static) {
    let guard = create_signal(()); // hook-owned: stable id + liveness across renders
    let id = guard.raw_id();
    TIMEOUTS.with(|t| {
        // `or_insert_with` so a re-render doesn't restart the countdown (the pending
        // timeout persists by hook position, like `create_loop`'s loop).
        t.borrow_mut().entry(id).or_insert_with(|| Timeout {
            at: None,
            delay: secs.max(0.0),
            action: Rc::new(f),
            guard: Some(guard),
        });
    });
    crate::reactive::create_cleanup(move || {
        TIMEOUTS.with(|t| {
            t.borrow_mut().remove(&id);
        });
    });
}

/// A non-hook one-shot timer keyed by a caller-supplied `id`: fires `f` once after
/// `secs`, then removes itself. Unlike [`create_timeout`] it is NOT tied to a component
/// — the caller owns its lifecycle and cancels early with [`clear_timeout`]. Registering
/// the same `id` again replaces the pending timer. For app services (toast auto-dismiss)
/// that schedule from an event handler rather than a component body.
pub fn set_timeout(id: u64, secs: f64, f: impl Fn() + 'static) {
    TIMEOUTS.with(|t| {
        t.borrow_mut()
            .insert(id, Timeout { at: None, delay: secs.max(0.0), action: Rc::new(f), guard: None });
    });
}

/// Cancel a pending [`set_timeout`] by its `id` (a no-op if it already fired/absent).
pub fn clear_timeout(id: u64) {
    TIMEOUTS.with(|t| {
        t.borrow_mut().remove(&id);
    });
}

/// The earliest absolute fire time (in the [`tick`] time base) among pending
/// timeouts, if any. The shell uses this to wake a waiting event loop exactly
/// when the next timer is due — otherwise hover timers (tooltips, hover cards)
/// would sleep until the next unrelated event.
pub fn next_deadline(now: f64) -> Option<f64> {
    TIMEOUTS.with(|t| {
        t.borrow()
            .values()
            .map(|to| to.at.unwrap_or(now + to.delay))
            .min_by(f64::total_cmp)
    })
}

/// A component hook: returns a value that cycles `0.0..1.0` every `period` seconds,
/// forever (until this component unmounts). The basis for spinners / indeterminate
/// indicators. Reading it re-renders the component every frame.
pub fn create_loop(period: f64) -> Signal<f64> {
    let value = create_signal(0.0_f64);
    let id = value.raw_id();
    LOOPS.with(|l| {
        l.borrow_mut().insert(id, Loop { value, period: period.max(0.05) });
    });
    crate::reactive::create_cleanup(move || {
        LOOPS.with(|l| {
            l.borrow_mut().remove(&id);
        });
    });
    value
}

/// Like [`create_loop`], but the loop ticks ONLY while `active` is true. The signal
/// (and its hook position) is stable across renders; the ticking entry is inserted or
/// removed to match `active` on each render. An inactive loop costs nothing — in
/// particular it does **not** keep the shell's frame loop running — so use this for
/// effects that should animate only in one state (a caret blinking only while its
/// field is focused).
pub fn create_loop_while(active: bool, period: f64) -> Signal<f64> {
    let value = create_signal(0.0_f64);
    let id = value.raw_id();
    LOOPS.with(|l| {
        let mut l = l.borrow_mut();
        if active {
            l.entry(id).or_insert(Loop { value, period: period.max(0.05) });
        } else {
            l.remove(&id);
        }
    });
    crate::reactive::create_cleanup(move || {
        LOOPS.with(|l| {
            l.borrow_mut().remove(&id);
        });
    });
    value
}

/// Advance all animations to `now` (monotonic seconds). Returns whether any remain
/// active. Called once per frame by the shell.
pub fn tick(now: f64) -> bool {
    let tweening = TRACKS.with(|t| {
        let mut t = t.borrow_mut();
        t.retain_mut(|tr| {
            // Drop tracks whose component unmounted mid-animation.
            if !tr.value.alive() {
                return false;
            }
            let start = *tr.start.get_or_insert(now);
            let elapsed = now - start;
            if elapsed >= tr.duration {
                tr.value.set(tr.to);
                false
            } else {
                let p = (elapsed / tr.duration).clamp(0.0, 1.0);
                let eased = tr.curve.apply(p);
                tr.value.set(tr.from + (tr.to - tr.from) * eased);
                true
            }
        });
        !t.is_empty()
    });
    let looping = LOOPS.with(|l| {
        let mut l = l.borrow_mut();
        // Drop loops whose component unmounted (else they spin forever, pinning the
        // frame loop). `create_cleanup` normally removes them; this is the backstop.
        l.retain(|_, lp| lp.value.alive());
        for lp in l.values() {
            lp.value.set((now / lp.period).rem_euclid(1.0));
        }
        !l.is_empty()
    });
    // Collect due timeouts and drop them from the map BEFORE firing (an action may
    // register another timeout / write signals — must not alias the borrow).
    let due: Vec<Rc<dyn Fn()>> = TIMEOUTS.with(|t| {
        let mut t = t.borrow_mut();
        let mut due = Vec::new();
        let mut done = Vec::new();
        for (id, to) in t.iter_mut() {
            if to.guard.is_some_and(|g| !g.alive()) {
                done.push(*id);
                continue;
            }
            let at = *to.at.get_or_insert(now + to.delay);
            if now >= at {
                due.push(to.action.clone());
                done.push(*id);
            }
        }
        for id in done {
            t.remove(&id);
        }
        due
    });
    for f in &due {
        f();
    }
    let timing_out = TIMEOUTS.with(|t| !t.borrow().is_empty());
    tweening || looping || timing_out
}

/// Reset the driver (used when tearing down between apps/tests).
pub fn reset() {
    TRACKS.with(|t| t.borrow_mut().clear());
    TIMEOUTS.with(|t| t.borrow_mut().clear());
}

/// Component hook: returns the current animated value that smoothly follows
/// `target`, starting a transition of `secs` seconds only when `target` changes.
///
/// Call it at the top level of a component (React rules of hooks). It owns two
/// local signals — the animated value and the last target — so successive renders
/// during a transition don't restart it.
pub fn animated(target: f64, secs: f64) -> f64 {
    animated_with(target, secs, Curve::EaseOutCubic)
}

/// Like [`animated`], with an explicit easing [`Curve`]. The basis for implicit
/// animations (`AnimatedContainer`-style) that need a curve other than the
/// default ease-out cubic.
pub fn animated_with(target: f64, secs: f64, curve: Curve) -> f64 {
    let value = create_signal(target);
    let last = create_signal(target);
    if (last.peek() - target).abs() > 1e-9 {
        last.set(target);
        animate_to_with(value, target, secs, curve);
    }
    value.get()
}
