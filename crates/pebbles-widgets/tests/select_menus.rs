//! Select & DropdownMenu polish (catalog 4.5): the SI-4 keyboard model drives the
//! open menus — arrows move (skipping disabled Select options), Enter picks, Escape
//! dismisses; disabled options are unpickable; the clearable ✕ resets the value
//! without opening the menu.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, KeyInput, Motion, Ui, component};
use pebbles_foundation::{CrossAxisAlignment, Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{
    OverlayHost, View, column, dropdown_menu, menu_item, overlay, select, select_item,
};

thread_local! {
    static PICKED: RefCell<Option<(usize, String)>> = const { RefCell::new(None) };
    static CLEARED: RefCell<bool> = const { RefCell::new(false) };
    static ACTION: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn down() -> KeyInput {
    KeyInput::Move { motion: Motion::Down, extend: false }
}

fn frame(ui: &mut Ui, env: &mut TextEnv, win: Size) {
    ui.rebuild_if_dirty();
    ui.layout(env, win);
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut scene);
}

fn tap(ui: &mut Ui, p: Offset) {
    ui.dispatch_pointer_down(p);
    ui.dispatch_tap(p);
    ui.dispatch_pointer_up(p);
}

fn select_root() -> impl IntoWidget {
    OverlayHost::wrap(
        column(vec![
            select([
                select_item("Apple"),
                select_item("Banana").disabled(true),
                select_item("Cherry"),
            ])
            .width(220.0)
            .on_changed(|i, l| PICKED.with(|p| *p.borrow_mut() = Some((i, l.to_string()))))
            .into_widget(),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch),
    )
}

#[test]
fn select_keyboard_skips_disabled_and_enter_picks() {
    overlay::init();
    pebbles_core::focus::init();
    PICKED.with(|p| *p.borrow_mut() = None);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 400.0);
    ui.mount_root(View::new(palette::WHITE, component(select_root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);

    // Open the menu (trigger top-left, packed to the start).
    tap(&mut ui, Offset::new(20.0, 18.0));
    frame(&mut ui, &mut env, win);
    assert!(overlay::is_open(), "tapping the trigger opens the menu");

    // Down → Apple (row 0); Down again skips the disabled Banana → Cherry; Enter.
    ui.dispatch_key(down());
    frame(&mut ui, &mut env, win);
    ui.dispatch_key(down());
    frame(&mut ui, &mut env, win);
    ui.dispatch_key(KeyInput::Enter);
    frame(&mut ui, &mut env, win);

    assert_eq!(
        PICKED.with(|p| p.borrow().clone()),
        Some((2, "Cherry".to_string())),
        "arrows skip the disabled option and Enter picks the active row"
    );
    assert!(!overlay::is_open(), "picking closes the menu");
}

#[test]
fn select_escape_dismisses_without_picking() {
    overlay::init();
    pebbles_core::focus::init();
    PICKED.with(|p| *p.borrow_mut() = None);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 400.0);
    ui.mount_root(View::new(palette::WHITE, component(select_root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);

    tap(&mut ui, Offset::new(20.0, 18.0));
    frame(&mut ui, &mut env, win);
    assert!(overlay::is_open());

    ui.dispatch_key(KeyInput::Escape);
    frame(&mut ui, &mut env, win);
    assert!(!overlay::is_open(), "Escape closes the menu");
    assert_eq!(PICKED.with(|p| p.borrow().clone()), None, "Escape picks nothing");
}

fn clear_root() -> impl IntoWidget {
    OverlayHost::wrap(
        column(vec![
            select(["One", "Two"])
                .width(220.0)
                .value(0)
                .clearable(true)
                .on_cleared(|| CLEARED.with(|c| *c.borrow_mut() = true))
                .into_widget(),
        ])
        // Start-aligned so the trigger keeps its 220px width (stretch would widen
        // it and move the ✕).
        .cross_axis_alignment(CrossAxisAlignment::Start),
    )
}

#[test]
fn select_clearable_x_resets_without_opening() {
    overlay::init();
    pebbles_core::focus::init();
    CLEARED.with(|c| *c.borrow_mut() = false);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 400.0);
    ui.mount_root(View::new(palette::WHITE, component(clear_root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);

    // The ✕ replaces the chevron at the trigger's right edge (width 220, padding
    // 12) while a value is selected.
    tap(&mut ui, Offset::new(200.0, 19.0));
    frame(&mut ui, &mut env, win);
    assert!(CLEARED.with(|c| c.borrow().clone()), "tapping the ✕ fires on_cleared");
    assert!(!overlay::is_open(), "the ✕ tap must not open the menu");
}

fn dd_root() -> impl IntoWidget {
    OverlayHost::wrap(
        column(vec![
            dropdown_menu("Open")
                .label("My Account")
                .item(menu_item("Profile").on_select(|| ACTION.with(|a| *a.borrow_mut() = Some("Profile".into()))))
                .item(menu_item("Billing").on_select(|| ACTION.with(|a| *a.borrow_mut() = Some("Billing".into()))))
                .separator()
                .item(menu_item("Log out").on_select(|| ACTION.with(|a| *a.borrow_mut() = Some("Log out".into()))))
                .into_widget(),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch),
    )
}

#[test]
fn dropdown_menu_keyboard_runs_actionable_rows() {
    overlay::init();
    pebbles_core::focus::init();
    ACTION.with(|a| *a.borrow_mut() = None);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 400.0);
    ui.mount_root(View::new(palette::WHITE, component(dd_root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);

    tap(&mut ui, Offset::new(20.0, 18.0));
    frame(&mut ui, &mut env, win);
    assert!(overlay::is_open(), "clicking the trigger opens the menu");

    // The label and separator are not navigable rows: Down → Profile, Down →
    // Billing, Enter runs Billing.
    ui.dispatch_key(down());
    frame(&mut ui, &mut env, win);
    ui.dispatch_key(down());
    frame(&mut ui, &mut env, win);
    ui.dispatch_key(KeyInput::Enter);
    frame(&mut ui, &mut env, win);

    assert_eq!(
        ACTION.with(|a| a.borrow().clone()),
        Some("Billing".to_string()),
        "Enter runs the second actionable row (labels/separators skipped)"
    );
    assert!(!overlay::is_open(), "running an action closes the menu");
}
