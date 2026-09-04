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

use crate::reactive::{create_effect, create_memo, create_signal, flush_effects};
use crate::reactive_stats as stats;

/// Flush the pending effect queue (what the shell does each frame via
/// `rebuild_if_dirty`), so a headless write settles its graph.
fn flush() {
    flush_effects();
}

/// Real leaf/component renders = effect runs minus memo recomputes. Today a memo
/// IS an effect, so its recompute is counted in `effect_runs`; subtracting the
/// memo recomputes leaves exactly the leaf effects that stood in for component
/// re-renders. (After T2 memos are their own node kind and this equals
/// `effect_runs` directly — the accounting still holds.)
fn leaf_renders() -> u64 {
    stats::effect_runs().saturating_sub(stats::memo_recomputes())
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
    stats::reset();
    let t = Instant::now();
    s.set(1);
    flush();
    let dt = t.elapsed();
    eprintln!("[reactive wide N={N}] {} ({dt:?})", stats::summary());
    assert_eq!(stats::notifies(), N as u64, "a write notifies exactly its readers");
    assert_eq!(stats::effect_runs(), N as u64, "all N readers re-run");
    // Baseline cost this tier kills: one box alloc + one scratch Vec per write.
    assert_eq!(stats::box_allocs(), 1);
    assert_eq!(stats::vec_allocs(), 1);
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
    // EAGER BASELINE: every memo recomputes, whether the tail was read or not.
    assert_eq!(stats::memo_recomputes(), K as u64, "eager: whole chain recomputes");
}

#[test]
fn shape_deep_chain_unread_is_wasted_work_baseline() {
    // The headline T2 target. Same deep chain but NOBODY reads the tail. In the
    // eager model every memo STILL recomputes (each is a standing effect). Lazy
    // push-pull will make this 0. This assertion flips at T2.
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
    assert_eq!(
        stats::memo_recomputes(),
        K as u64,
        "EAGER BASELINE: unread memos still recompute (T2 target: 0)"
    );
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
    assert_eq!(
        leaf_renders(),
        0,
        "the leaf does NOT render — the equality cut stopped the cascade"
    );
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
