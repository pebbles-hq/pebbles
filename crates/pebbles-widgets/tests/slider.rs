//! Slider behavior: dragging maps the pointer position to a value (nearest thumb),
//! and once focused the arrow keys step the active thumb while Home/End jump to the
//! domain ends. Driven headlessly through the same `Ui` path the shell uses.

use std::cell::RefCell;

use pebbles_core::keyboard::{KeyInput, Motion};
use pebbles_core::{IntoWidget, Ui, WidgetExt, component, create_signal};
use pebbles_foundation::{CrossAxisAlignment, Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{View, column, slider};

thread_local! {
    static LAST: RefCell<Vec<f64>> = const { RefCell::new(Vec::new()) };
}

fn record(v: Vec<f64>) {
    LAST.with(|l| *l.borrow_mut() = v);
}
fn value() -> f64 {
    LAST.with(|l| l.borrow().first().copied().unwrap_or(f64::NAN))
}

fn root() -> impl IntoWidget {
    // Fixed 200-wide, 0..=100 step-1 slider, pinned top-left so window x maps to it.
    let _ = create_signal(0i32); // give the component an owner scope
    column(vec![
        slider(200.0).min(0.0).max(100.0).step(1.0).value(0.0).on_changed(record).into_widget(),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .into_widget()
}

#[test]
fn drag_and_keyboard_move_the_thumb() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut text_env = TextEnv::new();
    let window = Size::new(400.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).boxed());
    ui.layout(&mut text_env, window);

    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut text_env, window);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut scene);
    };

    // A point on the slider track (height HIT = 20 → y = 10 is centered).
    let lo_pt = Offset::new(30.0, 10.0);
    let hi_pt = Offset::new(170.0, 10.0);

    let src = ui.pan_target_at(hi_pt).expect("pan target on the slider");

    // Drag near the right end → a high value.
    ui.dispatch_pan_start(src, hi_pt);
    frame(&mut ui);
    let v_high = value();
    assert!(v_high >= 80.0, "drag near right end should be high, got {v_high}");

    // Drag near the left end → a low value (nearest-thumb follows the pointer).
    ui.dispatch_pan_update(src, lo_pt);
    frame(&mut ui);
    let v_low = value();
    assert!(v_low <= 20.0, "drag near left end should be low, got {v_low}");
    assert!(v_high > v_low, "right drag ({v_high}) must exceed left drag ({v_low})");

    // The drag focused the slider — arrow Right steps up by one (step = 1).
    let before = value();
    ui.dispatch_key(KeyInput::Move { motion: Motion::Right, extend: false });
    frame(&mut ui);
    assert!((value() - (before + 1.0)).abs() < 1e-6, "Right should +1: {} → {}", before, value());

    // Arrow Left steps back down.
    let before = value();
    ui.dispatch_key(KeyInput::Move { motion: Motion::Left, extend: false });
    frame(&mut ui);
    assert!((value() - (before - 1.0)).abs() < 1e-6, "Left should -1: {} → {}", before, value());

    // Home / End jump to the domain ends.
    ui.dispatch_key(KeyInput::Move { motion: Motion::LineStart, extend: false });
    frame(&mut ui);
    assert_eq!(value(), 0.0, "Home jumps to min");
    ui.dispatch_key(KeyInput::Move { motion: Motion::LineEnd, extend: false });
    frame(&mut ui);
    assert_eq!(value(), 100.0, "End jumps to max");
}
