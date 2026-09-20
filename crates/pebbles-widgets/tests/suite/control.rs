//! Control-flow primitives (§2 blockers): `for_each` keyed churn and `suspense`.
//! `error_boundary` (build-time + descendant-render panic → fallback) already has
//! its own module (`error_boundary.rs`), so it isn't duplicated here.
//!
//! `for_each` is the list-diffing primitive — keying children by identity must
//! preserve element state across reorder/insert/remove instead of rebuilding by
//! position. We prove that with a per-item mount counter: reused elements never
//! re-mount, a genuine insert mounts exactly one.

use std::cell::{Cell, RefCell};

use pebbles_core::widget::AnyWidget;
use pebbles_core::{IntoWidget, Signal, component, component_props, create_signal, on_mount};
use pebbles_testing::Harness;
use pebbles_widgets::{column, for_each, suspense, text};

// ---------------------------------------------------------------------------
// for_each — keyed churn
// ---------------------------------------------------------------------------

thread_local! {
    static ITEMS: RefCell<Option<Signal<Vec<u64>>>> = const { RefCell::new(None) };
    static MOUNTS: Cell<u32> = const { Cell::new(0) };
}

fn items() -> Signal<Vec<u64>> {
    ITEMS.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            *c = Some(create_signal(vec![1u64, 2, 3]));
        }
        c.unwrap()
    })
}

struct ItemProps {
    id: u64,
}

fn item_render(p: &ItemProps) -> AnyWidget {
    // Fires once per element instance — a keyed reorder must NOT re-run this.
    on_mount(|| MOUNTS.with(|m| m.set(m.get() + 1)));
    text(format!("row {}", p.id)).into_widget()
}

fn item(id: u64) -> impl IntoWidget {
    component_props(item_render, ItemProps { id })
}

fn list_root() -> impl IntoWidget {
    let ids = items().get();
    column(for_each(ids, |id| *id, |id| item(*id)))
}

#[test]
fn for_each_keyed_churn_preserves_elements() {
    MOUNTS.with(|m| m.set(0));
    ITEMS.with(|c| *c.borrow_mut() = None); // fresh signal for this test
    let mut h = Harness::new();
    h.mount(|| component(list_root));
    h.draw();
    assert_eq!(MOUNTS.with(Cell::get), 3, "each of the 3 initial rows mounts once");

    // Reorder — every key survives, so nothing re-mounts.
    items().set(vec![3, 1, 2]);
    h.draw();
    assert_eq!(MOUNTS.with(Cell::get), 3, "a keyed reorder reuses elements (no new mounts)");

    // Insert a new key — exactly one new mount, the survivors are reused.
    items().set(vec![3, 1, 4, 2]);
    h.draw();
    assert_eq!(MOUNTS.with(Cell::get), 4, "inserting one key mounts exactly one element");

    // Remove keys — no new mounts, and the tree shrinks.
    items().set(vec![3]);
    h.draw();
    assert_eq!(MOUNTS.with(Cell::get), 4, "removing keys mounts nothing new");
    assert!(h.element_count() > 0, "the surviving row still renders");
}

// ---------------------------------------------------------------------------
// suspense — fallback while loading, content when resolved
// ---------------------------------------------------------------------------

thread_local! {
    static LOADING: RefCell<Option<Signal<bool>>> = const { RefCell::new(None) };
    static LAST_BUILT: RefCell<String> = const { RefCell::new(String::new()) };
}

fn loading() -> Signal<bool> {
    LOADING.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            *c = Some(create_signal(true));
        }
        c.unwrap()
    })
}

fn record(which: &str) {
    LAST_BUILT.with(|c| *c.borrow_mut() = which.to_string());
}

fn last_built() -> String {
    LAST_BUILT.with(|c| c.borrow().clone())
}

fn suspense_root() -> impl IntoWidget {
    suspense(
        || loading().get(),
        || {
            record("fallback");
            text("loading…")
        },
        || {
            record("content");
            text("ready")
        },
    )
}

#[test]
fn suspense_swaps_fallback_for_content_when_loading_clears() {
    LOADING.with(|c| *c.borrow_mut() = None); // fresh signal (default: loading)
    record("");
    let mut h = Harness::new();
    h.mount(|| component(suspense_root));
    h.draw();
    assert_eq!(last_built(), "fallback", "shows the fallback while loading");

    // Resource resolved.
    loading().set(false);
    h.draw();
    assert_eq!(last_built(), "content", "swaps to content when loading clears");

    // …and back (a re-fetch).
    loading().set(true);
    h.draw();
    assert_eq!(last_built(), "fallback", "returns to the fallback on reload");
}
