//! Regression: picking a time (opening the dropdown, scrolling, hovering a slot
//! mid-animation, then picking) must never crash — the exact user report.

use pebbles_core::{IntoWidget, KeyInput, Ui, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{OverlayHost, View, column, time_field};

fn root() -> impl IntoWidget {
    OverlayHost::wrap(column(vec![time_field().width(200.0).into_widget()]))
}

#[test]
fn picking_a_time_never_crashes() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    let window = Size::new(400.0, 600.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut text, window);

    let mut now = 0.0_f64;
    // The shell's per-frame work, in order, including a real CPU-side paint.
    let frame = |ui: &mut Ui, text: &mut TextEnv, now: &mut f64| {
        *now += 0.016;
        pebbles_core::animation::tick(*now);
        ui.tick_scrolls(0.016);
        ui.rebuild_if_dirty();
        ui.layout(text, window);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(text, &mut scene);
    };

    let chevron = Offset::new(182.0, 19.0); // the dropdown-open button

    let field_body = Offset::new(70.0, 19.0); // the editable input area

    for _ in 0..6 {
        // Focus the editable field and type a partial time (as a user would).
        ui.dispatch_pointer_down(field_body);
        ui.dispatch_tap(field_body);
        ui.dispatch_pointer_up(field_body);
        frame(&mut ui, &mut text, &mut now);
        ui.dispatch_key(KeyInput::Insert("0930".to_string()));
        frame(&mut ui, &mut text, &mut now);

        // Open the time dropdown.
        ui.dispatch_tap(chevron);
        frame(&mut ui, &mut text, &mut now);
        frame(&mut ui, &mut text, &mut now);

        // Hover a slot, let its highlight spring run a few frames, then pick it —
        // unmounting the whole menu while a slot's animation is in flight.
        let slot = Offset::new(70.0, 90.0);
        ui.dispatch_hover(slot);
        frame(&mut ui, &mut text, &mut now);
        frame(&mut ui, &mut text, &mut now);
        ui.dispatch_pointer_down(slot);
        ui.dispatch_tap(slot);
        ui.dispatch_pointer_up(slot);
        for _ in 0..4 {
            frame(&mut ui, &mut text, &mut now);
        }
        // Hover where a slot used to be (fires the stale exit path if any).
        ui.dispatch_hover(Offset::new(76.0, 96.0));
        frame(&mut ui, &mut text, &mut now);

        // Reopen, fling-scroll the long list, then pick mid-scroll.
        ui.dispatch_tap(chevron);
        frame(&mut ui, &mut text, &mut now);
        frame(&mut ui, &mut text, &mut now);
        ui.dispatch_scroll(Offset::new(70.0, 140.0), 300.0);
        frame(&mut ui, &mut text, &mut now);
        let slot2 = Offset::new(70.0, 140.0);
        ui.dispatch_hover(slot2);
        frame(&mut ui, &mut text, &mut now);
        ui.dispatch_pointer_down(slot2);
        ui.dispatch_tap(slot2);
        ui.dispatch_pointer_up(slot2);
        for _ in 0..4 {
            frame(&mut ui, &mut text, &mut now);
        }
        // After picking (value set from outside), keep typing into the focused field.
        ui.dispatch_key(KeyInput::Insert("1".to_string()));
        ui.dispatch_key(KeyInput::Backspace);
        frame(&mut ui, &mut text, &mut now);
    }
}
