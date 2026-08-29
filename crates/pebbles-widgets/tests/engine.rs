//! Headless integration test for the widget engine: mount → layout → tap →
//! reconcile → relayout, all without a window or GPU. This exercises the entire
//! Flutter-style pipeline end to end.

use pebbles_core::{
    AnyWidget, BuildContext, IntoWidget, State, StatefulWidget, Ui, Widget, WidgetExt,
    stateful_widget,
};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::{RenderConstrainedBox, TextEnv};
use pebbles_widgets::{GestureDetector, SizedBox, View, center};

/// A probe widget whose visible size encodes how many times it has been tapped,
/// so the test can observe the full setState → rebuild → relayout loop by reading
/// the render tree.
#[derive(Clone)]
struct Probe;

stateful_widget!(Probe);

impl StatefulWidget for Probe {
    fn create_state(&self) -> Box<dyn State> {
        Box::new(ProbeState { taps: 0 })
    }
}

struct ProbeState {
    taps: i64,
}

impl State for ProbeState {
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

    fn build(&mut self, _widget: &dyn Widget, cx: &mut BuildContext) -> AnyWidget {
        let bump = cx.callback(|s: &mut ProbeState| s.taps += 1);
        // `center` fills the window (so a tap anywhere hits the detector); the inner
        // childless SizedBox's width == 10 + taps*10 is what we assert on.
        GestureDetector::new(center(SizedBox::spacer(10.0 + self.taps as f64 * 10.0, 10.0)))
            .on_tap(bump)
            .into_widget()
    }
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

    ui.mount_root(View::new(palette::WHITE, Probe.into_widget()).boxed());
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
        View::new(palette::WHITE, center(SizedBox::spacer(10.0, 10.0))).boxed(),
    );
    ui.layout(&mut text, Size::new(100.0, 100.0));
    // No GestureDetector in this tree → nothing handles the tap.
    assert!(!ui.dispatch_tap(Offset::new(50.0, 50.0)));
}
