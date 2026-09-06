//! Reactive stress harness (R0): builds the canonical graph shapes — wide, deep,
//! diamond, fan — writes a signal, flushes, and reads the
//! [`reactive_stats`](crate::reactive_stats) counters. These are the measuring
//! stick for the state-management work: the assertions document the CURRENT
//! (eager) behavior and become the regression guards the later tiers flip
//! (a lazy memo that nobody reads must recompute 0 times, not once).
//!
//! An app-scoped `create_effect` stands in for "a component re-render" — same
//! reactive semantics (it is the leaf that must run when its inputs change),
//! without needing a `Ui`/window. Run with `--nocapture` to read the numbers.

use std::time::Instant;

use crate::reactive::stats;
use crate::reactive::{create_effect, create_memo, create_signal, flush_effects};

/// Flush the pending effect queue (what the shell does each frame via
/// `rebuild_if_dirty`), so a headless write settles its graph.
fn flush() {
    flush_effects();
}

/// Real leaf/component renders. Since T2, memos are their own node kind (not
/// effects), so `effect_runs` counts exactly the leaf effects that stand in for
/// component re-renders.
fn leaf_renders() -> u64 {
    stats::effect_runs()
}

#[test]
fn shape_wide_one_signal_many_readers() {
    // 1 signal → N leaf effects. A write wakes exactly N readers.
    const N: usize = 500;
    let s = create_signal(0i64);
    for _ in 0..N {
        create_effect(move || {
            let _ = s.get();
        });
    }
    // Warm-up write: grows the scheduler scratch to its peak so the MEASURED write
    // below reflects steady-state cost (T1 makes it allocation-free).
    s.set(1);
    flush();
    stats::reset();
    let t = Instant::now();
    s.set(2);
    flush();
    let dt = t.elapsed();
    eprintln!("[reactive wide N={N}] {} ({dt:?})", stats::summary());
    assert_eq!(stats::notifies(), N as u64, "a write notifies exactly its readers");
    assert_eq!(stats::effect_runs(), N as u64, "all N readers re-run");
    // T1: a steady-state write allocates NOTHING — the box is reused, the scratch
    // buffers are recycled (was 1 box + 1 Vec per write at the R0 baseline).
    assert_eq!(stats::box_allocs(), 0, "the value box is reused, not re-allocated");
    assert_eq!(stats::vec_allocs(), 0, "the scheduler scratch is recycled");
}

#[test]
fn shape_deep_chain_recompute_count() {
    // signal → memo¹⁰⁰ → leaf effect. One write cascades through the chain.
    const K: usize = 100;
    let s = create_signal(0i64);
    let mut prev = create_memo(move || s.get() + 1);
    for _ in 1..K {
        let p = prev;
        prev = create_memo(move || p.get() + 1);
    }
    let tail = prev;
    create_effect(move || {
        let _ = tail.get();
    });
    stats::reset();
    let t = Instant::now();
    s.set(1);
    flush();
    let dt = t.elapsed();
    eprintln!("[reactive deep K={K}] {} ({dt:?})", stats::summary());
    // The tail IS read, so the whole chain is demanded — every memo recomputes
    // exactly once (lazy, but fully pulled).
    assert_eq!(stats::memo_recomputes(), K as u64, "a fully-read chain recomputes once each");
}

#[test]
fn shape_deep_chain_unread_is_wasted_work_baseline() {
    // The headline T2 property. Same deep chain but NOBODY reads the tail: a write
    // recomputes NOTHING (lazy push-pull). The eager baseline was K recomputes.
    const K: usize = 100;
    let s = create_signal(0i64);
    let mut prev = create_memo(move || s.get() + 1);
    for _ in 1..K {
        let p = prev;
        prev = create_memo(move || p.get() + 1);
    }
    let _unread = prev; // never read by any effect/component
    stats::reset();
    s.set(1);
    flush();
    eprintln!("[reactive deep-unread K={K}] {}", stats::summary());
    // T2 WIN: an unread memo chain recomputes NOTHING — a write only flips flags.
    assert_eq!(stats::memo_recomputes(), 0, "lazy: unread memos do not recompute (was {K} eager)");
}

#[test]
fn shape_diamond_no_double_render_and_equal_cut() {
    // signal → {a, b} → leaf. The leaf must NOT double-render, and if both memos
    // recompute EQUAL the leaf must not render at all (the equality-cut firewall).
    let s = create_signal(0i64);
    let a = create_memo(move || s.get() * 2);
    let b = create_memo(move || s.get() + 10);
    create_effect(move || {
        let _ = a.get();
        let _ = b.get();
    });
    // A real change: both memos change, the leaf renders ONCE (dedup).
    stats::reset();
    s.set(1);
    flush();
    eprintln!("[reactive diamond change] {} leaf_renders={}", stats::summary(), leaf_renders());
    assert_eq!(stats::memo_recomputes(), 2, "both memos recompute");
    assert_eq!(leaf_renders(), 1, "the leaf renders once, not twice (no double render)");

    // An equal write path: force both memos to recompute to the SAME value. Using
    // a memo whose output is constant regardless of the input proves the cut.
    let s2 = create_signal(0i64);
    let ca = create_memo(move || {
        let _ = s2.get();
        42
    });
    let cb = create_memo(move || {
        let _ = s2.get();
        7
    });
    create_effect(move || {
        let _ = ca.get();
        let _ = cb.get();
    });
    stats::reset();
    s2.set(999);
    flush();
    eprintln!("[reactive diamond equal-cut] {} leaf_renders={}", stats::summary(), leaf_renders());
    assert_eq!(stats::memo_recomputes(), 2, "both memos recompute (eager)");
    assert_eq!(leaf_renders(), 0, "the leaf does NOT render — the equality cut stopped the cascade");
}

#[test]
fn shape_fan_many_signals_one_memo() {
    // N signals → 1 memo → leaf. One signal write recomputes the memo once.
    const N: usize = 200;
    let sigs: Vec<_> = (0..N).map(|i| create_signal(i as i64)).collect();
    let inputs = sigs.clone();
    let m = create_memo(move || inputs.iter().map(|s| s.get()).sum::<i64>());
    create_effect(move || {
        let _ = m.get();
    });
    stats::reset();
    sigs[0].set(1_000);
    flush();
    eprintln!("[reactive fan N={N}] {} leaf_renders={}", stats::summary(), leaf_renders());
    assert_eq!(stats::memo_recomputes(), 1, "one input write = one memo recompute");
    assert_eq!(leaf_renders(), 1, "the leaf renders once");
}

#[test]
fn lazy_memo_recomputes_on_read_not_on_write() {
    // A memo with NO standing reader. A write must not recompute it; the next
    // READ must (pull-on-demand), returning the fresh value.
    let s = create_signal(2i64);
    let m = create_memo(move || s.get() * 10);
    assert_eq!(m.peek(), 20);
    stats::reset();
    s.set(5);
    flush();
    assert_eq!(stats::memo_recomputes(), 0, "a write does not recompute an unread memo");
    // Reading it now pulls it — exactly one recompute — and yields the fresh value.
    let v = m.peek();
    assert_eq!(v, 50, "the pull returns the up-to-date value");
    assert_eq!(stats::memo_recomputes(), 1, "the read recomputed it once");
    // A second read with no intervening write does NOT recompute (it's Clean).
    let _ = m.peek();
    assert_eq!(stats::memo_recomputes(), 1, "a clean memo is not recomputed on re-read");
}

#[test]
fn diamond_reads_are_glitch_free() {
    // A leaf that reads two memos of the SAME signal must always see them
    // CONSISTENT (both reflect the same input) — never a half-updated (new, old)
    // pair. The two-phase settle (memos before leaves) guarantees this.
    use std::cell::Cell;
    use std::rc::Rc;
    let s = create_signal(1i64);
    let a = create_memo(move || s.get() * 2);
    let b = create_memo(move || s.get() * 2);
    let seen: Rc<Cell<(i64, i64)>> = Rc::new(Cell::new((0, 0)));
    let seen2 = seen.clone();
    create_effect(move || {
        seen2.set((a.get(), b.get()));
    });
    assert_eq!(seen.get(), (2, 2));
    for input in [2, 3, 10, 4] {
        s.set(input);
        flush();
        let (va, vb) = seen.get();
        assert_eq!(va, vb, "the leaf saw a glitched (a != b) pair for input {input}");
        assert_eq!(va, input * 2, "the leaf saw the up-to-date value");
    }
}

#[test]
fn store_select_memo_is_lazy_and_field_scoped() {
    use crate::reactive::create_store;
    use std::cell::Cell;
    use std::rc::Rc;
    #[derive(Clone)]
    struct State {
        a: i64,
        b: i64,
    }
    let store = create_store(State { a: 1, b: 100 });
    // A selector on `a` only. Its reader (a leaf effect) must re-run ONLY when `a`
    // changes — a write that touches only `b` must not wake it (field-scoped via
    // the lazy equality-cut memo).
    let a_sel = store.select_memo(|s| s.a);
    let runs = Rc::new(Cell::new(0u32));
    let r2 = runs.clone();
    create_effect(move || {
        let _ = a_sel.get();
        r2.set(r2.get() + 1);
    });
    assert_eq!(runs.get(), 1, "the selector's reader ran once at mount");

    // Write only `b`: the store notifies, but the `a` selector cuts (a unchanged)
    // → its reader does NOT re-run.
    store.update(|s| s.b = 200);
    flush();
    assert_eq!(runs.get(), 1, "a write to an untouched field does not wake the selector");

    // Write `a`: the selector changes → its reader re-runs exactly once.
    store.update(|s| s.a = 2);
    flush();
    assert_eq!(runs.get(), 2, "a write to the selected field wakes the reader once");
}

#[test]
fn memo_with_reference_policy_cuts_on_pointer_identity() {
    use crate::reactive::create_memo_with;
    // A memo over Rc<i64> that cuts on pointer identity: recomputing to the SAME
    // Rc must not wake readers even though the Rc is cloned each time.
    let s = create_signal(0i64);
    let shared = std::rc::Rc::new(7i64);
    let shared2 = shared.clone();
    let m = create_memo_with(
        move || {
            let _ = s.get(); // depends on s, but always returns the same Rc
            shared2.clone()
        },
        std::rc::Rc::ptr_eq,
    );
    let runs = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let r2 = runs.clone();
    create_effect(move || {
        let _ = m.get();
        r2.set(r2.get() + 1);
    });
    assert_eq!(runs.get(), 1);
    s.set(1);
    flush();
    assert_eq!(runs.get(), 1, "pointer-identical recompute does not wake the reader");
    let _ = shared; // keep the original Rc alive
}

#[test]
fn on_tracks_only_deps_and_defer_skips_first() {
    use crate::reactive::{on, on_defer};
    use std::cell::Cell;
    use std::rc::Rc;
    let dep = create_signal(0i64);
    let noise = create_signal(0i64);
    let ran = Rc::new(Cell::new(0u32));
    let r2 = ran.clone();
    on(
        move || dep.get(),
        move |d| {
            // Body reads `noise` but must NOT subscribe to it.
            let _ = noise.peek();
            r2.set(r2.get() + 1);
            let _ = d;
        },
    );
    assert_eq!(ran.get(), 1, "on runs once at mount");
    noise.set(5); // not a dependency → no re-run
    flush();
    assert_eq!(ran.get(), 1, "on ignores reads its body made (untracked body)");
    dep.set(1);
    flush();
    assert_eq!(ran.get(), 2, "on re-runs when its dep changes");

    // on_defer skips the mount run.
    let dep2 = create_signal(0i64);
    let ran2 = Rc::new(Cell::new(0u32));
    let r3 = ran2.clone();
    on_defer(move || dep2.get(), move |_| r3.set(r3.get() + 1));
    assert_eq!(ran2.get(), 0, "on_defer does not run at mount");
    dep2.set(1);
    flush();
    assert_eq!(ran2.get(), 1, "on_defer runs on the first change");
}

#[test]
fn effect_cascade_through_lazy_memo_converges() {
    // An effect writes a signal that feeds a memo that another effect reads: the
    // settle-then-run flush loop must converge to the final consistent value
    // (T4.3 reentrancy/ordering under the lazy marking).
    use std::cell::Cell;
    use std::rc::Rc;
    let trigger = create_signal(0i64);
    let mid = create_signal(0i64);
    // Effect A: mirrors `trigger` into `mid` (a write feeding the memo below).
    create_effect(move || {
        let t = trigger.get();
        mid.set(t * 10);
    });
    let doubled = create_memo(move || mid.get() * 2);
    let seen = Rc::new(Cell::new(-1i64));
    let s2 = seen.clone();
    create_effect(move || {
        s2.set(doubled.get());
    });
    assert_eq!(seen.get(), 0);
    trigger.set(3);
    flush();
    // trigger=3 → mid=30 → doubled=60. The cascade settled to the final value.
    assert_eq!(seen.get(), 60, "the effect→signal→memo→effect cascade converged");
}

#[test]
fn memo_with_changing_dependencies_resubscribes_correctly() {
    // A memo that reads DIFFERENT signals depending on a toggle. When its sources
    // change, the recompute's source diff must unsubscribe the dropped input and
    // subscribe the new one — so the old input stops waking it and the new one
    // starts. Exercises the add/remove branch of the zero-churn source tracking.
    let toggle = create_signal(true);
    let a = create_signal(1i64);
    let b = create_signal(100i64);
    let m = create_memo(move || if toggle.get() { a.get() } else { b.get() });
    assert_eq!(m.peek(), 1);
    stats::reset();

    // While reading `a`: writing `b` must NOT recompute (not a source).
    b.set(200);
    flush();
    assert_eq!(stats::memo_recomputes(), 0, "b is not a source while the toggle reads a");
    // Writing `a` recomputes.
    a.set(2);
    flush();
    assert_eq!(m.peek(), 2);

    // Flip to read `b`: the memo's sources change (a → toggle+b).
    stats::reset();
    toggle.set(false);
    flush();
    let _ = m.peek(); // pull to settle the new sources
    assert_eq!(m.peek(), 200, "now reads b");
    // `a` is no longer a source: writing it must NOT recompute the memo.
    stats::reset();
    a.set(999);
    flush();
    assert_eq!(stats::memo_recomputes(), 0, "a was unsubscribed — it no longer wakes the memo");
    // `b` IS a source now: writing it recomputes.
    b.set(300);
    flush();
    assert_eq!(m.peek(), 300, "b now drives the memo");
}
