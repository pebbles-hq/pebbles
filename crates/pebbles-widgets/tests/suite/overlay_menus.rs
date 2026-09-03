//! Regression: the overlay-backed dropdowns (Select, DropdownMenu, Combobox,
//! MultiSelect) must never crash as they open, hover, pick, toggle and close —
//! all of which mount and unmount hover-listening rows in the overlay layer.
//! Sweep-taps the whole surface with a full frame after each event, exactly like
//! the shell, so any use-after-free of a freed signal would panic here.

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{CrossAxisAlignment, Offset, Size, palette};
use pebbles_render::{RenderDecoratedBox, TextEnv};
use pebbles_widgets::{
    OverlayHost, View, column, combobox, dropdown_menu, menu_item, multi_select, select, time_field,
};

fn root() -> impl IntoWidget {
    let body = column(vec![
        select(["Free", "Pro", "Enterprise", "Team"]).width(200.0).into_widget(),
        dropdown_menu("Menu")
            .label("Group")
            .item(menu_item("Profile").on_select(|| {}))
            .item(menu_item("Billing").on_select(|| {}))
            .separator()
            .item(menu_item("Delete").destructive().on_select(|| {}))
            .check("Toggle", false, |_| {})
            .into_widget(),
        combobox(["Apple", "Banana", "Cherry", "Date", "Elderberry"]).width(200.0).into_widget(),
        multi_select(["Red", "Green", "Blue", "Cyan"]).width(200.0).into_widget(),
        time_field().width(200.0).into_widget(),
    ]);
    OverlayHost::wrap(body)
}

/// The DropdownMenu's default trigger must be bounded to its configured width
/// with the chevron pushed to the right edge — not stretched across the whole
/// row. Regression for "the trigger is full screen / the arrow isn't at the right".
#[test]
fn dropdown_trigger_is_bounded_width() {
    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    // A deliberately wide window: a full-width trigger would measure ~900, not 220.
    ui.mount_root(
        View::new(
            palette::WHITE,
            column(vec![
                dropdown_menu("Open menu")
                    .width(220.0)
                    .item(menu_item("A").on_select(|| {}))
                    .into_widget(),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start),
        )
        .into_widget(),
    );
    ui.layout(&mut text, Size::new(900.0, 600.0));

    let tree = ui.render_tree();
    let boxid = tree.find::<RenderDecoratedBox>().expect("the trigger's bordered box");
    let w = tree.size_of(boxid).width;
    assert!(
        (w - 220.0).abs() < 0.5,
        "trigger should be its 220px width, got {w} (full-window bug if it's ~900)"
    );
}

#[test]
fn overlay_menus_never_crash() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    let window = Size::new(360.0, 560.0);

    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut text, window);

    let mut now = 0.0_f64;
    let frame = |ui: &mut Ui, text: &mut TextEnv, now: &mut f64| {
        ui.rebuild_if_dirty();
        *now += 0.016;
        pebbles_core::animation::tick(*now);
        ui.layout(text, window);
    };

    for _ in 0..3 {
        let mut y = 8.0;
        while y < 420.0 {
            let mut x = 8.0;
            while x < 320.0 {
                let p = Offset::new(x, y);
                // Hover (starts a row's spring), open/pick, then hover away — the
                // sequence that fired a stale exit handler on an unmounted row.
                ui.dispatch_hover(p);
                frame(&mut ui, &mut text, &mut now);
                ui.dispatch_pointer_down(p);
                ui.dispatch_tap(p);
                ui.dispatch_pointer_up(p);
                frame(&mut ui, &mut text, &mut now);
                ui.dispatch_hover(Offset::new(x + 7.0, y + 7.0));
                frame(&mut ui, &mut text, &mut now);
                x += 18.0;
            }
            y += 18.0;
        }
    }
}
