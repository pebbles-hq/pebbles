//! Debug-only reactive-runtime counters: how much work a write actually caused
//! this flush — signal writes, subscriber notifies, memo recomputes, effect runs,
//! component schedules, and the two hot-path allocations (a value re-box in `set`,
//! a scratch buffer in the scheduler).
//!
//! These are the measuring stick for the state-management work (research says a
//! write must do the minimum work and nothing recomputes until it is pulled). All
//! counters compile to no-ops in release — the same pattern as
//! `pebbles_render::stats` and the E6c lifecycle census. `PEBBLES_REACTIVE_STATS=1`
//! makes the shell print them; tests read them directly.
//!
//! Counters are cumulative; call [`reset`] to zero them around a measured window.

#[cfg(debug_assertions)]
thread_local! {
    static WRITES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static NOTIFIES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static MEMO_RECOMPUTES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static EFFECT_RUNS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static COMPONENT_SCHEDULES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static BOX_ALLOCS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static VEC_ALLOCS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Zero every counter (call around a measured window).
pub fn reset() {
    #[cfg(debug_assertions)]
    {
        WRITES.with(|c| c.set(0));
        NOTIFIES.with(|c| c.set(0));
        MEMO_RECOMPUTES.with(|c| c.set(0));
        EFFECT_RUNS.with(|c| c.set(0));
        COMPONENT_SCHEDULES.with(|c| c.set(0));
        BOX_ALLOCS.with(|c| c.set(0));
        VEC_ALLOCS.with(|c| c.set(0));
    }
}

macro_rules! counter {
    ($name:ident, $bump:ident, $get:ident, $doc:literal) => {
        #[inline]
        pub(crate) fn $bump() {
            #[cfg(debug_assertions)]
            $name.with(|c| c.set(c.get() + 1));
        }
        #[doc = $doc]
        pub fn $get() -> u64 {
            #[cfg(debug_assertions)]
            {
                $name.with(std::cell::Cell::get)
            }
            #[cfg(not(debug_assertions))]
            {
                0
            }
        }
    };
}

counter!(WRITES, bump_write, writes, "Signal writes that actually changed the value.");
counter!(NOTIFIES, bump_notify, notifies, "Subscribers notified across all writes.");
counter!(MEMO_RECOMPUTES, bump_memo_recompute, memo_recomputes, "Memo function evaluations.");
counter!(EFFECT_RUNS, bump_effect_run, effect_runs, "Effect function runs.");
counter!(
    COMPONENT_SCHEDULES,
    bump_component_schedule,
    component_schedules,
    "Components scheduled to re-render."
);
counter!(BOX_ALLOCS, bump_box_alloc, box_allocs, "Value box (re)allocations in `set`.");
counter!(VEC_ALLOCS, bump_vec_alloc, vec_allocs, "Scratch `Vec` allocations in the scheduler.");

/// A one-line summary of the current counters (for `PEBBLES_REACTIVE_STATS=1`).
pub fn summary() -> String {
    format!(
        "writes={} notifies={} memo_recomputes={} effect_runs={} component_schedules={} \
         box_allocs={} vec_allocs={}",
        writes(),
        notifies(),
        memo_recomputes(),
        effect_runs(),
        component_schedules(),
        box_allocs(),
        vec_allocs(),
    )
}
