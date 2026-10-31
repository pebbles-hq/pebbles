//! Draggable / DragTarget: a press only becomes a drag past the slop (Flutter's
//! Draggable waits for its recognizer to win), so a click inside a draggable — a tab's
//! close button, say — never starts a drag or drops the payload.

use std::cell::Cell;

use pebbles_core::IntoWidget;
use pebbles_foundation::Offset;
use pebbles_testing::Harness;
use pebbles_widgets::{container, drag_target, draggable, row, text};

thread_local! {
    static DROPS: Cell<usize> = const { Cell::new(0) };
}

fn dnd_root() -> impl IntoWidget {
    row(vec![
        draggable(7u32, container().width(100.0).height(40.0).child(text("Chip"))).into_widget(),
        drag_target(|_| container().width(100.0).height(40.0).into_widget())
            .on_accept::<u32>(|_| DROPS.with(|d| d.set(d.get() + 1)))
            .into_widget(),
    ])
}

#[test]
fn a_press_without_movement_is_not_a_drag_and_a_drag_past_the_slop_drops() {
    let mut h = Harness::new().window(300.0, 100.0);
    h.mount(dnd_root);
    h.frames(3);
    let on_chip = Offset::new(50.0, 50.0);
    let on_target = Offset::new(150.0, 50.0);
    assert!(h.ui.pan_target_at(on_chip).is_some(), "the draggable claims the pan");

    // Press, jitter under the gesture layer's pan slop, release: a click — the
    // draggable never sees a pan start, so no feedback and no drop.
    h.drag(on_chip, Offset::new(53.0, 52.0));
    assert_eq!(h.find_all_text("Chip").len(), 1, "no drag feedback on a press");
    assert_eq!(DROPS.with(Cell::get), 0, "no drag under the slop");

    // A real drag onto the target drops once.
    h.drag(on_chip, on_target);
    assert_eq!(DROPS.with(Cell::get), 1, "a drag past the slop drops on the target");
}
