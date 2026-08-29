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

use crate::reactive::{Signal, create_signal};

#[derive(Clone, Copy)]
struct Track {
    value: Signal<f64>,
    from: f64,
    to: f64,
    /// Filled on the first tick that sees this track (so timing starts at paint).
    start: Option<f64>,
    duration: f64,
}

/// A continuous, repeating driver (spinners, indeterminate progress). Its signal
/// cycles `0.0..1.0` every `period` seconds until the owning component unmounts.
struct Loop {
    value: Signal<f64>,
    period: f64,
}

thread_local! {
    static TRACKS: RefCell<Vec<Track>> = const { RefCell::new(Vec::new()) };
    static LOOPS: RefCell<std::collections::HashMap<u64, Loop>> =
        RefCell::new(std::collections::HashMap::new());
}

/// Ease-out cubic — fast start, gentle settle. The default UI curve.
fn ease_out_cubic(t: f64) -> f64 {
    let f = 1.0 - t;
    1.0 - f * f * f
}

/// Animate `value` from its current reading to `to` over `duration` seconds. A new
/// call to the same signal replaces any in-flight animation (smooth reversal). A
/// zero-distance change snaps immediately.
pub fn animate_to(value: Signal<f64>, to: f64, duration: f64) {
    let from = value.peek();
    TRACKS.with(|t| {
        let mut t = t.borrow_mut();
        t.retain(|tr| tr.value != value);
        if duration <= 0.0 || (from - to).abs() <= 1e-4 {
            value.set(to);
        } else {
            t.push(Track { value, from, to, start: None, duration });
        }
    });
}

/// Whether any animation (tween or loop) is currently running (the shell polls
/// this to decide whether to schedule another frame).
pub fn active() -> bool {
    TRACKS.with(|t| !t.borrow().is_empty()) || LOOPS.with(|l| !l.borrow().is_empty())
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
                let eased = ease_out_cubic(p);
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
    tweening || looping
}

/// Reset the driver (used when tearing down between apps/tests).
pub fn reset() {
    TRACKS.with(|t| t.borrow_mut().clear());
}

/// Component hook: returns the current animated value that smoothly follows
/// `target`, starting a transition of `secs` seconds only when `target` changes.
///
/// Call it at the top level of a component (React rules of hooks). It owns two
/// local signals — the animated value and the last target — so successive renders
/// during a transition don't restart it.
pub fn animated(target: f64, secs: f64) -> f64 {
    let value = create_signal(target);
    let last = create_signal(target);
    if (last.peek() - target).abs() > 1e-9 {
        last.set(target);
        animate_to(value, target, secs);
    }
    value.get()
}
