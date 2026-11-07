//! `PEBBLES_REBUILD_PROFILE=1`: time every dirty component's re-render (inclusive of
//! its reconciled subtree) and let the shell print the frame's costliest ones — finds a
//! signal read too high in the tree (a whole panel re-rendering for a hover) without a
//! sampling profiler. Off: one cached env check per dirty component.

use std::cell::RefCell;
use std::sync::OnceLock;
use std::time::Duration;

static FORCED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the profiler records (`PEBBLES_REBUILD_PROFILE=1`, dev mode `PEBBLES_DEV=1` —
/// for the "re-renders every frame" warning — or [`set_enabled`]).
pub fn enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    FORCED.load(std::sync::atomic::Ordering::Relaxed)
        || *ON.get_or_init(|| printing() || crate::log::dev_mode())
}

/// Whether to print every frame's profile (`PEBBLES_REBUILD_PROFILE=1`).
pub fn printing() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("PEBBLES_REBUILD_PROFILE").is_ok_and(|v| v == "1"))
}

thread_local! {
    /// Consecutive frames each component re-rendered in, and whether it was reported.
    static STREAK: RefCell<std::collections::HashMap<(&'static str, Site), (u32, bool)>> =
        RefCell::new(std::collections::HashMap::new());
}

/// Frames in a row a component may re-render before the dev warning (2 s at 60 Hz).
pub const EVERY_FRAME_WARN: u32 = 120;

/// Fold one frame's self-time rows into the per-component streaks; returns the components
/// that just reached [`EVERY_FRAME_WARN`] consecutive frames (each reported once per
/// streak) — a signal read in a component that changes every frame (a scroll offset, an
/// animation value) where only a leaf should re-render.
pub fn note_frame(rows: &[(&'static str, Site, u32, Duration)]) -> Vec<String> {
    STREAK.with(|m| {
        let mut m = m.borrow_mut();
        let seen: std::collections::HashSet<(&'static str, Site)> = rows.iter().map(|r| (r.0, r.1)).collect();
        m.retain(|k, _| seen.contains(k));
        let mut out = Vec::new();
        for k in seen {
            let e = m.entry(k).or_insert((0, false));
            e.0 += 1;
            if e.0 >= EVERY_FRAME_WARN && !e.1 {
                e.1 = true;
                out.push(label(k.0, k.1));
            }
        }
        out
    })
}

/// Turn the profiler on/off from code (perf tests count component renders with it).
pub fn set_enabled(on: bool) {
    FORCED.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Where a component was created (`component_props(…)` call site): `file:line`.
pub type Site = Option<&'static std::panic::Location<'static>>;

/// `name @ file:line` for a profiler row.
pub fn label(name: &str, site: Site) -> String {
    match site {
        Some(l) => format!("{name} @ {}:{}", l.file(), l.line()),
        None => name.to_string(),
    }
}

thread_local! {
    static ROWS: RefCell<Vec<(&'static str, Site, Duration)>> = const { RefCell::new(Vec::new()) };
    /// Per-component SELF time this frame (renders, mounts, updates; children excluded).
    static SELF: RefCell<std::collections::HashMap<(&'static str, Site), (u32, Duration)>> =
        RefCell::new(std::collections::HashMap::new());
    /// Child time accumulated under each open component run.
    static STACK: RefCell<Vec<Duration>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn record(name: &'static str, site: Site, took: Duration) {
    ROWS.with(|r| r.borrow_mut().push((name, site, took)));
}

/// Start timing one component run (render + its child reconcile).
pub(crate) fn enter() -> Option<std::time::Instant> {
    if !enabled() {
        return None;
    }
    STACK.with(|s| s.borrow_mut().push(Duration::ZERO));
    Some(std::time::Instant::now())
}

/// Finish a run started by [`enter`]: book its self time (total minus nested runs).
pub(crate) fn exit(t0: Option<std::time::Instant>, name: &'static str, site: Site) {
    let Some(t0) = t0 else { return };
    let total = t0.elapsed();
    let nested = STACK.with(|s| s.borrow_mut().pop()).unwrap_or_default();
    STACK.with(|s| {
        if let Some(parent) = s.borrow_mut().last_mut() {
            *parent += total;
        }
    });
    SELF.with(|m| {
        let mut m = m.borrow_mut();
        let e = m.entry((name, site)).or_insert((0, Duration::ZERO));
        e.0 += 1;
        e.1 += total.saturating_sub(nested);
    });
}

/// This frame's per-component self time `(name, creation site, runs, self time)`, costliest
/// first; clears it.
pub fn take_self() -> Vec<(&'static str, Site, u32, Duration)> {
    let mut v: Vec<_> = SELF
        .with(|m| std::mem::take(&mut *m.borrow_mut()))
        .into_iter()
        .map(|((n, a), (c, d))| (n, a, c, d))
        .collect();
    v.sort_by(|a, b| b.3.cmp(&a.3));
    v
}

/// This frame's dirty-component renders `(name, creation site, inclusive time)`,
/// costliest first; clears the list.
pub fn take() -> Vec<(&'static str, Site, Duration)> {
    let mut v = ROWS.with(|r| std::mem::take(&mut *r.borrow_mut()));
    v.sort_by(|a, b| b.2.cmp(&a.2));
    v
}

