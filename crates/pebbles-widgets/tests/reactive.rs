//! Headless proof of the SolidJS-style reactive loop: a function component reads a
//! signal, a plain-closure tap handler writes it, and the framework re-renders the
//! component and reconciles — all without a window or GPU.

use pebbles_core::{Element, IntoWidget, Ui, action, component, create_signal};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::{RenderConstrainedBox, TextEnv};
use pebbles_widgets::{GestureDetector, SizedBox, View, center};

/// A component whose visible width encodes how many times it was tapped.
fn probe() -> Element {
    let taps = create_signal(0);
    let bump = action(move || taps.update(|t| *t += 1));
    GestureDetector::new(center(SizedBox::new(Some(10.0 + taps.get() as f64 * 10.0), Some(10.0), None)))
        .on_tap(bump)
        .into_widget()
}

fn probe_width(ui: &Ui) -> f64 {
    let tree = ui.render_tree();
    let id = tree.find::<RenderConstrainedBox>().expect("probe SizedBox present");
    tree.size_of(id).width
}

#[test]
fn signal_write_re_renders_component() {
    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    let window = Size::new(200.0, 200.0);

    ui.mount_root(View::new(palette::WHITE, component(probe)).into_widget());
    ui.layout(&mut text, window);
    assert_eq!(probe_width(&ui), 10.0, "initial: taps == 0");

    // A tap fires the plain closure → signal.update → schedules the component.
    assert!(ui.dispatch_tap(Offset::new(100.0, 100.0)), "tap handled");
    assert!(ui.rebuild_if_dirty(), "signal write marks the component dirty");
    ui.layout(&mut text, window);
    assert_eq!(probe_width(&ui), 20.0, "after one tap: taps == 1");

    for _ in 0..3 {
        assert!(ui.dispatch_tap(Offset::new(100.0, 100.0)));
        ui.rebuild_if_dirty();
    }
    ui.layout(&mut text, window);
    assert_eq!(probe_width(&ui), 50.0, "after four taps: taps == 4");
}
