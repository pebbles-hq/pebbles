//! Headless integration test for the widget engine: mount → layout → tap →
//! reconcile → relayout, all without a window or GPU. This exercises the entire
//! Flutter-style pipeline end to end.

use pebbles_core::{IntoWidget, Ui, component, create_signal};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::{RenderConstrainedBox, TextEnv};
use pebbles_widgets::{SizedBox, View, center, gesture_detector};

/// A probe component whose visible size encodes how many times it has been tapped,
/// so the test can observe the full signal-write → rebuild → relayout loop by reading
/// the render tree.
fn probe() -> impl IntoWidget {
    let taps = create_signal(0i64);
    // `center` fills the window (so a tap anywhere hits the detector); the inner
    // childless SizedBox's width == 10 + taps*10 is what we assert on.
    gesture_detector(center(SizedBox::new(Some(10.0 + taps.get() as f64 * 10.0), Some(10.0), None)))
        .on_tap(move || taps.update(|t| *t += 1))
}

fn probe_box_width(ui: &Ui) -> f64 {
    let tree = ui.render_tree();
    let id = tree.find::<RenderConstrainedBox>().expect("probe SizedBox present");
    tree.size_of(id).width
}

#[test]
fn tap_drives_setstate_reconcile_and_relayout() {
    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    let window = Size::new(200.0, 200.0);

    ui.mount_root(View::new(palette::WHITE, component(probe)).into_widget());
    ui.layout(&mut text, window);

    // Initial state: taps == 0 → width 10.
    assert_eq!(probe_box_width(&ui), 10.0);

    // Tap at the center — the full-window GestureDetector must handle it.
    assert!(ui.dispatch_tap(Offset::new(100.0, 100.0)), "tap should be handled");

    // The shell would do this each frame: reconcile dirty subtrees, then relayout.
    assert!(ui.rebuild_if_dirty(), "a tap marks the element dirty");
    ui.layout(&mut text, window);

    // After one tap: taps == 1 → width 20.
    assert_eq!(probe_box_width(&ui), 20.0);

    // Three more taps.
    for _ in 0..3 {
        assert!(ui.dispatch_tap(Offset::new(100.0, 100.0)));
        ui.rebuild_if_dirty();
    }
    ui.layout(&mut text, window);
    assert_eq!(probe_box_width(&ui), 50.0);
}

#[test]
fn tap_outside_any_listener_is_unhandled() {
    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    ui.mount_root(
        View::new(palette::WHITE, center(SizedBox::new(Some(10.0), Some(10.0), None))).into_widget(),
    );
    ui.layout(&mut text, Size::new(100.0, 100.0));
    // No GestureDetector in this tree → nothing handles the tap.
    assert!(!ui.dispatch_tap(Offset::new(50.0, 50.0)));
}

// Keyed reconciliation: a child's state must follow its KEY across a reorder, not
// stay at its position. Without keyed matching (index-based), a reorder rebuilds
// children in place and the state stays put (or resets) — this asserts it moves.
#[test]
fn keyed_children_preserve_state_across_reorder() {
    use std::cell::Cell;

    use pebbles_core::Signal;
    use pebbles_widgets::{column, keyed};

    thread_local! {
        static ORDER: Cell<Option<Signal<bool>>> = const { Cell::new(None) };
        static A_WIDTH: Cell<Option<Signal<f64>>> = const { Cell::new(None) };
    }

    // Item "a": a box whose width is local signal state we can grow.
    fn item_a() -> impl IntoWidget {
        let w = create_signal(10.0_f64);
        A_WIDTH.with(|c| c.set(Some(w)));
        SizedBox::new(Some(w.get()), Some(10.0), None)
    }
    // Item "b": a plain 10×10 box.
    fn item_b() -> impl IntoWidget {
        SizedBox::new(Some(10.0), Some(10.0), None)
    }
    // Root: two keyed children whose order is a signal we flip.
    fn root() -> impl IntoWidget {
        let order = create_signal(false);
        ORDER.with(|c| c.set(Some(order)));
        let a = keyed("a", component(item_a));
        let b = keyed("b", component(item_b));
        let kids = if order.get() { vec![b, a] } else { vec![a, b] };
        column(kids)
    }

    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    let window = Size::new(200.0, 200.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut text, window);

    // The wide item's column slot = its Keyed wrapper's local y. `offset_of` is
    // local, so the inner box sits at y=0 inside the wrapper; the slot is the MAX
    // local y among the wide (110) boxes.
    let wide_slot_y = |ui: &Ui| -> f64 {
        let t = ui.render_tree();
        t.find_all::<RenderConstrainedBox>()
            .into_iter()
            .filter(|id| (t.size_of(*id).width - 110.0).abs() < 0.5)
            .map(|id| t.offset_of(id).y)
            .fold(f64::NEG_INFINITY, f64::max)
    };

    // Grow item "a" to 110 wide (its local state). It sits in the TOP slot (y == 0).
    A_WIDTH.with(|c| c.get().unwrap()).set(110.0);
    ui.rebuild_if_dirty();
    ui.layout(&mut text, window);
    assert_eq!(wide_slot_y(&ui), 0.0, "the grown item starts in the top slot");

    // Flip the order → "a" moves to the BOTTOM slot. With keyed matching its element
    // (and the 110 width) follows the key down; without keys it would remount to 10
    // and there'd be no wide box at all.
    ORDER.with(|c| c.get().unwrap()).set(true);
    ui.rebuild_if_dirty();
    ui.layout(&mut text, window);
    assert!(wide_slot_y(&ui).is_finite(), "the grown item still exists (state preserved)");
    assert_eq!(wide_slot_y(&ui), 10.0, "the grown item followed its key to the bottom slot");
}

// AnimatedList keeps a removed item alive through its exit tween, then drops it —
// and new items appear. Driven headlessly with the animation clock + timeout firing.
#[test]
fn animated_list_enters_holds_exit_then_drops() {
    use std::cell::RefCell;

    use pebbles_core::{AnyWidget, Signal, animation, create_signal};
    use pebbles_widgets::{SizedBox, animated_list};

    thread_local! {
        static KEYS: RefCell<Option<Signal<Vec<u64>>>> = const { RefCell::new(None) };
    }

    // key k → a k*100-wide, 20-tall box, so each item is identifiable by width.
    fn shell() -> impl IntoWidget {
        let keys = KEYS.with(|c| c.borrow().expect("KEYS set"));
        let items: Vec<(u64, AnyWidget)> = keys
            .get()
            .iter()
            .map(|&k| (k, SizedBox::new(Some(k as f64 * 100.0), Some(20.0), None).into_widget()))
            .collect();
        animated_list(items).duration(0.2)
    }

    KEYS.with(|c| *c.borrow_mut() = None);
    let keys = create_signal(vec![1u64, 2u64]);
    KEYS.with(|c| *c.borrow_mut() = Some(keys));

    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    let win = Size::new(400.0, 400.0);
    ui.mount_root(View::new(palette::WHITE, component(shell)).into_widget());
    ui.layout(&mut text, win);

    let has = |ui: &Ui, w: f64| {
        let t = ui.render_tree();
        t.find_all::<RenderConstrainedBox>().into_iter().any(|id| (t.size_of(id).width - w).abs() < 0.5)
    };
    let mut frame = |ui: &mut Ui, now: f64| {
        animation::tick(now);
        ui.rebuild_if_dirty();
        ui.layout(&mut text, win);
    };

    // Both items entered.
    frame(&mut ui, 0.1);
    frame(&mut ui, 0.3);
    assert!(has(&ui, 100.0) && has(&ui, 200.0), "both items present after enter");

    // Remove key 2 → it must stay for its exit tween (not vanish instantly).
    keys.set(vec![1]);
    frame(&mut ui, 0.35);
    assert!(has(&ui, 200.0), "the removed item stays alive during its exit tween");

    // Past the exit duration the removal timeout fires → item 2 is dropped.
    frame(&mut ui, 0.7);
    frame(&mut ui, 0.9);
    assert!(!has(&ui, 200.0), "the removed item is gone after its exit");
    assert!(has(&ui, 100.0), "the surviving item remains");

    // Add a new key 3 → it appears (enters).
    keys.set(vec![1, 3]);
    frame(&mut ui, 1.0);
    frame(&mut ui, 1.3);
    assert!(has(&ui, 300.0), "a newly added item appears");
}
