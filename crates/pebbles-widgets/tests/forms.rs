//! Widget-catalog forms batch: Field (3.6) labeled-control composite and ToggleGroup
//! (3.5) single/multi-select. Field is verified by painting; ToggleGroup's selection
//! logic is verified by driving taps through a real Ui.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, Ui, WidgetExt, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{OverlayHost, View, field, text_field, toggle_group_labels};

#[test]
fn field_paints_label_control_and_error() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    fn root() -> impl IntoWidget {
        // error_opt(Some) must replace the description with the red message.
        field(text_field().width(200.0))
            .label("Email")
            .description("We never share it.")
            .error_opt(Some("Required"))
    }

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(root)).boxed());
    ui.layout(&mut env, Size::new(300.0, 200.0));
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut scene); // must lay out + paint the label/control/error stack, no panic
}

thread_local! {
    static SEL: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

fn group_root() -> impl IntoWidget {
    // Single-select group; report the picked indices.
    OverlayHost::wrap(
        toggle_group_labels(["Left", "Center", "Right"])
            .value(0)
            .on_changed(|sel| SEL.with(|c| *c.borrow_mut() = sel.to_vec())),
    )
}

#[test]
fn toggle_group_single_select_picks_one() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    SEL.with(|c| c.borrow_mut().clear());

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let window = Size::new(400.0, 120.0);
    ui.mount_root(View::new(palette::WHITE, component(group_root)).boxed());
    ui.layout(&mut env, window);
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, window);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut scene);
    };
    frame(&mut ui);

    // The three cells sit left-to-right; tap within the second cell. Cells are ~48–64px
    // wide with 6px gaps starting at x=0, y≈20. The middle cell is comfortably past x=90.
    let mid = Offset::new(110.0, 18.0);
    ui.dispatch_pointer_down(mid);
    ui.dispatch_tap(mid);
    ui.dispatch_pointer_up(mid);
    frame(&mut ui);

    assert_eq!(SEL.with(|c| c.borrow().clone()), vec![1], "tapping the middle cell selects index 1");
}
