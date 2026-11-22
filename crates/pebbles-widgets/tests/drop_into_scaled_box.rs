//! A drop target inside a scaled-down `FittedBox` receives a drop where it is drawn.
//!
//! Regression: published bounds (`use_bounds`) ignored ancestor transforms, so a target in a
//! scaled frame (the Gravel UI builder's device preview) registered its unscaled rect and a
//! drop over it was delivered to whatever target's unscaled rect happened to lie under the
//! pointer.

use std::cell::Cell;
use std::rc::Rc;

use pebbles_core::IntoWidget;
use pebbles_foundation::{BoxFit, Offset};
use pebbles_testing::Harness;
use pebbles_widgets::{column, container, drag_target, end_drag, fitted_box, start_drag, update_drag};

thread_local! {
    static TOP: Cell<u32> = const { Cell::new(0) };
    static BOTTOM: Cell<u32> = const { Cell::new(0) };
}

/// An 800-tall page scaled down into a 400-tall box (scale 0.5): the top target is drawn at
/// y 0..200, the bottom one at y 200..400.
fn page() -> impl IntoWidget {
    let target = |hit: &'static std::thread::LocalKey<Cell<u32>>| {
        drag_target(|_| container().width(400.0).height(400.0).into_widget())
            .on_will_accept::<u8>(|_| true)
            .on_accept::<u8>(move |_| hit.with(|c| c.set(c.get() + 1)))
            .into_widget()
    };
    container().width(400.0).height(400.0).child(fitted_box(column(vec![target(&TOP), target(&BOTTOM)])).fit(BoxFit::Contain))
}

#[test]
fn a_drop_lands_on_the_target_drawn_under_the_pointer_in_a_scaled_box() {
    let mut h = Harness::new().window(400.0, 400.0);
    h.mount(page);
    h.frames(3);
    // Drop at y 300: drawn inside the bottom target (its unscaled rect would be y 400..800,
    // and the top one's unscaled 0..400 would wrongly contain the pointer).
    start_drag(Rc::new(1u8), Offset::new(100.0, 300.0), Offset::ZERO, container().into_widget(), 10.0, 10.0);
    update_drag(Offset::new(100.0, 300.0));
    h.frames(2);
    assert!(end_drag(), "a target took the drop");
    assert_eq!((TOP.with(Cell::get), BOTTOM.with(Cell::get)), (0, 1), "delivered to the target drawn under the pointer");
}
