//! Memo equality dedup (checklist 1.6): a `create_memo` over a *coarse* projection of
//! a signal only re-renders its downstream reader when the projection actually flips,
//! not on every input write. Driven headlessly through the real reconcile loop.

use std::cell::{Cell, RefCell};

use pebbles_core::{Element, IntoWidget, Signal, Ui, component, create_memo, create_signal};
use pebbles_foundation::{Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{SizedBox, View};

thread_local! {
    static SOURCE: RefCell<Option<Signal<i32>>> = const { RefCell::new(None) };
    static PARITY: RefCell<Option<Signal<bool>>> = const { RefCell::new(None) };
    static RENDERS: Cell<u32> = const { Cell::new(0) };
}

fn source() -> Signal<i32> {
    SOURCE.with(|c| c.borrow().expect("source created before mount"))
}
fn parity() -> Signal<bool> {
    PARITY.with(|c| c.borrow().expect("memo created before mount"))
}

/// Reads the memo (a coarse `is_even` projection) and counts its own renders.
fn child() -> Element {
    RENDERS.with(|c| c.set(c.get() + 1));
    let even = parity().get(); // subscribe to the memo, not the raw source
    SizedBox::new(Some(if even { 10.0 } else { 20.0 }), Some(10.0), None).into_widget()
}

#[test]
fn memo_dedups_downstream_rerenders() {
    // Source + memo are app-owned (created before mount), like real global state.
    let src = create_signal(0i32);
    SOURCE.with(|c| *c.borrow_mut() = Some(src));
    let par = create_memo(move || src.get() % 2 == 0); // coarse: even/odd only
    PARITY.with(|c| *c.borrow_mut() = Some(par));

    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    let window = Size::new(100.0, 100.0);
    ui.mount_root(View::new(palette::WHITE, component(child)).into_widget());
    ui.layout(&mut text, window);
    assert_eq!(RENDERS.with(Cell::get), 1, "one render on mount");

    // 0 → 2: still even, so the memo's value is unchanged → child must NOT re-render.
    source().set(2);
    ui.rebuild_if_dirty();
    ui.layout(&mut text, window);
    assert_eq!(RENDERS.with(Cell::get), 1, "unchanged projection skips the re-render");

    // 2 → 1: even → odd, the projection flips → child re-renders once.
    source().set(1);
    ui.rebuild_if_dirty();
    ui.layout(&mut text, window);
    assert_eq!(RENDERS.with(Cell::get), 2, "a flipped projection re-renders");

    // 1 → 3: still odd → no re-render.
    source().set(3);
    ui.rebuild_if_dirty();
    ui.layout(&mut text, window);
    assert_eq!(RENDERS.with(Cell::get), 2, "still-odd projection skips the re-render");

    // 3 → 4: odd → even, flips again → one more render.
    source().set(4);
    ui.rebuild_if_dirty();
    ui.layout(&mut text, window);
    assert_eq!(RENDERS.with(Cell::get), 3, "flipped back re-renders");
}

#[test]
fn set_if_changed_swallows_noop_writes() {
    let s = create_signal(7i32);
    assert!(!s.set_if_changed(7), "writing the same value is a no-op");
    assert!(s.set_if_changed(8), "writing a different value reports changed");
    assert_eq!(s.peek(), 8);
}
