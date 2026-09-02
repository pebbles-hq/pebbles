//! E4: `Store::select_memo` — a deduped slice selector. Writing a field the selector
//! ignores must not re-render the reader; changing the selected slice must. Driven
//! through the real reconcile loop (mirrors `tests/memo.rs`).

use std::cell::{Cell, RefCell};

use pebbles_core::{Element, IntoWidget, Store, Ui, component, create_store};
use pebbles_foundation::{Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{SizedBox, View};

#[derive(Clone, Copy)]
struct State {
    a: i32,
    b: i32,
}

thread_local! {
    static STORE: RefCell<Option<Store<State>>> = const { RefCell::new(None) };
    static RENDERS: Cell<u32> = const { Cell::new(0) };
}

fn store() -> Store<State> {
    STORE.with(|c| c.borrow().expect("store created before mount"))
}

/// Selects only `a`; reads it (subscribing to the memo, not the whole store).
fn child() -> Element {
    RENDERS.with(|c| c.set(c.get() + 1));
    let a = store().select_memo(|s| s.a);
    SizedBox::new(Some(10.0 + a.get() as f64), Some(10.0), None).into_widget()
}

#[test]
fn select_memo_dedups_on_unselected_writes() {
    let st = create_store(State { a: 0, b: 0 });
    STORE.with(|c| *c.borrow_mut() = Some(st));
    RENDERS.with(|c| c.set(0));

    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    let window = Size::new(100.0, 100.0);
    ui.mount_root(View::new(palette::WHITE, component(child)).into_widget());
    ui.layout(&mut text, window);
    assert_eq!(RENDERS.with(Cell::get), 1, "one render on mount");

    // Write an UNSELECTED field (b) → selected slice `a` unchanged → no re-render.
    store().update(|s| s.b = 99);
    ui.rebuild_if_dirty();
    ui.layout(&mut text, window);
    assert_eq!(RENDERS.with(Cell::get), 1, "writing b (unselected) doesn't re-render");

    // Write the SELECTED field (a) → slice changes → one re-render.
    store().update(|s| s.a = 1);
    ui.rebuild_if_dirty();
    ui.layout(&mut text, window);
    assert_eq!(RENDERS.with(Cell::get), 2, "changing a re-renders");

    // Write a to the SAME value → deduped → no re-render.
    store().update(|s| s.a = 1);
    ui.rebuild_if_dirty();
    ui.layout(&mut text, window);
    assert_eq!(RENDERS.with(Cell::get), 2, "no-op write to a is deduped");
}
