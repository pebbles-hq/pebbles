//! C7: an open DropdownMenu announces its panel as a `Menu` and each row as a
//! `MenuItem` (checkable rows carry their checked state). Overlay-backed, so this
//! mirrors the select_menus harness: mount an `OverlayHost`, tap the trigger to
//! open the panel, then read the semantics tree.

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{CrossAxisAlignment, Offset, Size, palette};
use pebbles_render::{SemanticsRole, TextEnv};
use pebbles_widgets::{OverlayHost, View, column, dropdown_menu, menu_item, overlay};
use pebbles_testing::{draw_frame as frame};

fn tap(ui: &mut Ui, p: Offset) {
    ui.dispatch_pointer_down(p);
    ui.dispatch_tap(p);
    ui.dispatch_pointer_up(p);
}

fn menu_root() -> impl IntoWidget {
    OverlayHost::wrap(
        column(vec![
            dropdown_menu("Menu")
                .width(220.0)
                .item(menu_item("Profile").on_select(|| {}))
                .item(menu_item("Billing").on_select(|| {}))
                .check("Word Wrap", true, |_| {})
                .into_widget(),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch),
    )
}

#[test]
fn open_dropdown_announces_menu_and_menuitems() {
    overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 400.0);
    ui.mount_root(View::new(palette::WHITE, component(menu_root)).into_widget());
    frame(&mut ui, &mut env, win);

    // Open the menu (the trigger is packed top-left).
    tap(&mut ui, Offset::new(20.0, 18.0));
    frame(&mut ui, &mut env, win);
    assert!(overlay::is_open(), "tapping the trigger opens the menu");

    let tree = ui.render_tree().semantics_tree();
    assert!(tree.iter().any(|n| n.props.role == SemanticsRole::Menu), "the open panel is a Menu");

    let items: Vec<_> = tree.iter().filter(|n| n.props.role == SemanticsRole::MenuItem).collect();
    assert!(items.iter().any(|n| n.props.label == "Profile"), "Profile row is a MenuItem");
    assert!(items.iter().any(|n| n.props.label == "Billing"), "Billing row is a MenuItem");

    let wrap = items
        .iter()
        .find(|n| n.props.label == "Word Wrap")
        .expect("the check row is a MenuItem");
    assert_eq!(wrap.props.checked, Some(true), "a checkable row reports its checked state");
}
