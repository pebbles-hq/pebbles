//! Accessibility semantics tree (checklist 1.1): interactive widgets annotate their
//! subtree, and `RenderTree::semantics_tree()` yields a flat list of nodes (role,
//! label, state, window-space bounds) that the shell maps onto AccessKit. Verified
//! headlessly — no screen reader or platform needed to prove the tree is correct.

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Size, palette};
use pebbles_render::{SemanticsRole, TextEnv};
use pebbles_widgets::{View, button, checkbox, column, semantics, switch, text, text_field};

fn root() -> impl IntoWidget {
    column(vec![
        button("Save").into_widget(),
        checkbox(true).label("Accept terms").into_widget(),
        switch(false).label("Notifications").into_widget(),
        text_field().label("Email").into_widget(),
        // A custom control annotated by hand.
        semantics(SemanticsRole::Slider, "Volume", text("volume")).value("7").into_widget(),
    ])
    .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start)
}

#[test]
fn interactive_widgets_populate_the_semantics_tree() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, Size::new(400.0, 400.0));

    let tree = ui.render_tree().semantics_tree();

    // Every interactive widget contributed a node with the right role + label.
    let by_role = |r: SemanticsRole| tree.iter().find(|n| n.props.role == r);

    let btn = by_role(SemanticsRole::Button).expect("button node");
    assert_eq!(btn.props.label, "Save");
    assert!(!btn.props.disabled);

    let cb = by_role(SemanticsRole::Checkbox).expect("checkbox node");
    assert_eq!(cb.props.label, "Accept terms");
    assert_eq!(cb.props.checked, Some(true));

    let sw = by_role(SemanticsRole::Switch).expect("switch node");
    assert_eq!(sw.props.label, "Notifications");
    assert_eq!(sw.props.checked, Some(false));

    let tf = by_role(SemanticsRole::TextInput).expect("text input node");
    assert_eq!(tf.props.label, "Email");

    let sl = by_role(SemanticsRole::Slider).expect("hand-annotated slider node");
    assert_eq!(sl.props.label, "Volume");
    assert_eq!(sl.props.value.as_deref(), Some("7"));

    // Nodes carry real window-space bounds (laid out, non-empty, stacked vertically).
    assert!(btn.bounds.width() > 0.0 && btn.bounds.height() > 0.0, "button has bounds");
    assert!(cb.bounds.y0 > btn.bounds.y0, "checkbox is below the button in the column");
}

fn locked_button() -> impl IntoWidget {
    button("Locked").disabled(true)
}

#[test]
fn disabled_state_is_reported() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(locked_button)).into_widget());
    ui.layout(&mut env, Size::new(200.0, 100.0));

    let tree = ui.render_tree().semantics_tree();
    let btn = tree.iter().find(|n| n.props.role == SemanticsRole::Button).expect("button");
    assert_eq!(btn.props.label, "Locked");
    assert!(btn.props.disabled, "a disabled button reports disabled to a11y");
}
