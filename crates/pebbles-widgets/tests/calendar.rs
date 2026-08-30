//! Regression: switching the calendar's caption dropdowns (day ↔ month ↔ year
//! views) must never crash. Reproduces the "select the month dropdown → panic"
//! report by sweep-tapping the whole calendar and running a full frame (reconcile
//! + animation tick + relayout) after every tap, exactly as the shell would.

use pebbles_core::{IntoWidget, Ui, WidgetExt};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{CaptionLayout, View, calendar};

#[test]
fn calendar_view_switching_never_crashes() {
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    let window = Size::new(320.0, 460.0);

    ui.mount_root(
        View::new(palette::WHITE, calendar(|_, _, _| {}).caption(CaptionLayout::Dropdown).into_widget())
            .boxed(),
    );
    ui.layout(&mut text, window);

    // Sweep the calendar surface with taps. This hits the month/year caption
    // "dropdowns" (switching to the month grid and year grid), the grid cells
    // (switching back to days), the nav arrows, and the day cells — every path
    // that mounts/unmounts a sub-panel. A frame runs after each tap so any
    // use-after-free of a freed signal would panic here.
    let mut now = 0.0_f64;
    let frame = |ui: &mut Ui, text: &mut TextEnv, now: &mut f64| {
        ui.rebuild_if_dirty();
        *now += 0.016;
        pebbles_core::animation::tick(*now);
        ui.layout(text, window);
    };

    for _ in 0..2 {
        let mut y = 10.0;
        while y < 290.0 {
            let mut x = 10.0;
            while x < 252.0 {
                let p = Offset::new(x, y);
                // Hover to start the target's spring animation, and let it run a
                // frame so a live animation track exists across the unmount.
                ui.dispatch_hover(p);
                frame(&mut ui, &mut text, &mut now);
                // Click: switches the panel and unmounts the hovered (animating) button.
                ui.dispatch_pointer_down(p);
                ui.dispatch_tap(p);
                ui.dispatch_pointer_up(p);
                frame(&mut ui, &mut text, &mut now);
                // Hover moves onto whatever replaced the unmounted widget — this is
                // the step that fired the stale exit handler before the fix.
                ui.dispatch_hover(Offset::new(x + 6.0, y + 6.0));
                frame(&mut ui, &mut text, &mut now);
                x += 16.0;
            }
            y += 16.0;
        }
    }
}
