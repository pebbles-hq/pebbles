//! A transformed widget must still be hit-testable — the pointer point is mapped
//! through the inverse transform. Also guards that the hit-test rewrite left
//! untransformed hit-testing unchanged.

use pebbles_core::{IntoWidget, Ui, action};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{gesture_detector, SizedBox, Transform, View};

fn hit(tapped: &std::rc::Rc<std::cell::Cell<bool>>) -> impl IntoWidget {
    let t = tapped.clone();
    // A 40×40 tap target at the top-left, shifted +100px in x by the transform.
    Transform::translate(
        100.0,
        0.0,
        gesture_detector(SizedBox::new(Some(40.0), Some(40.0), None)).on_tap(action(move || t.set(true))),
    )
    .alignment(pebbles_foundation::Alignment::TOP_LEFT)
}

#[test]
fn translated_widget_is_hit_at_its_translated_position() {
    let tapped = std::rc::Rc::new(std::cell::Cell::new(false));
    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, hit(&tapped)).into_widget());
    ui.layout(&mut text, Size::new(400.0, 400.0));

    // The box's *untransformed* location (0..40) is now empty — a tap there misses.
    assert!(!ui.dispatch_tap(Offset::new(20.0, 20.0)), "original position should be empty");
    assert!(!tapped.get());

    // Its *translated* location (100..140) receives the tap.
    assert!(ui.dispatch_tap(Offset::new(120.0, 20.0)), "translated position should hit");
    assert!(tapped.get(), "tap handler should have fired through the transform");
}
