//! Select & DropdownMenu polish (catalog 4.5): the SI-4 keyboard model drives the
//! open menus — arrows move (skipping disabled Select options), Enter picks, Escape
//! dismisses; disabled options are unpickable; the clearable ✕ resets the value
//! without opening the menu.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, KeyInput, Motion, Ui, animation, component};
use pebbles_foundation::{CrossAxisAlignment, Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_testing::{draw_frame as frame};
use pebbles_widgets::{
    OverlayHost, View, column, dropdown_menu, menu_item, menu_sub, overlay, select, select_item,
};

thread_local! {
    static PICKED: RefCell<Option<(usize, String)>> = const { RefCell::new(None) };
    static CLEARED: RefCell<bool> = const { RefCell::new(false) };
    static ACTION: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn down() -> KeyInput {
    KeyInput::Move { motion: Motion::Down, extend: false }
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

fn sub_root() -> impl IntoWidget {
    OverlayHost::wrap(
        column(vec![
            dropdown_menu("Open")
                .label("My Account")
                .item(menu_item("Profile"))
                .item(menu_item("Billing"))
                .item(menu_item("Settings"))
                .item(menu_sub(
                    "Share",
                    [
                        menu_item("Copy link").on_select(|| ACTION.with(|a| *a.borrow_mut() = Some("Copied link".into()))),
                        menu_item("Invite teammates").on_select(|| ACTION.with(|a| *a.borrow_mut() = Some("Invite sent".into()))),
                    ],
                ))
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

#[test]
fn submenu_opens_on_hover_and_closes_after_grace() {
    overlay::init();
    pebbles_core::focus::init();
    animation::reset();
    ACTION.with(|a| *a.borrow_mut() = None);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(600.0, 400.0);
    overlay::set_window_size(600.0, 400.0);
    ui.mount_root(View::new(palette::WHITE, component(sub_root)).into_widget());
    ui.layout(&mut env, win);
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, win);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut env, &mut scene);
    };
    frame(&mut ui);

    // Open the menu (trigger at the top-left; menu anchors below it).
    tap(&mut ui, Offset::new(20.0, 18.0));
    frame(&mut ui);
    assert!(overlay::is_open());

    // The sub row: panel top (44) + pad (4) + label (28) + 3 items (96) → the row
    // spans y ≈ 172..204; the child panel opens to its right at (244, 168) — but
    // only after the 0.25s hover delay.
    ui.dispatch_hover(Offset::new(100.0, 188.0));
    frame(&mut ui);
    assert!(!overlay::child_is_open(), "not open before the hover delay");
    animation::tick(0.01);
    animation::tick(0.30);
    frame(&mut ui);
    assert!(overlay::child_is_open(), "opens after the hover delay");

    // Moving onto the child panel (≈ x 244..444) keeps it open…
    ui.dispatch_hover(Offset::new(300.0, 190.0));
    frame(&mut ui);
    assert!(overlay::child_is_open(), "the child stays open while hovered");

    // …and leaving both arms the grace-close (0.3s).
    ui.dispatch_hover(Offset::new(480.0, 10.0));
    frame(&mut ui);
    assert!(overlay::child_is_open(), "not closed before the grace delay");
    animation::tick(0.01);
    animation::tick(0.35);
    frame(&mut ui);
    assert!(!overlay::child_is_open(), "closed after the grace delay");

    // Reopen the submenu and pick its first item: the action runs and the whole
    // overlay closes.
    ui.dispatch_hover(Offset::new(100.0, 188.0));
    frame(&mut ui);
    animation::tick(0.01);
    animation::tick(0.30);
    frame(&mut ui);
    assert!(overlay::child_is_open());
    tap(&mut ui, Offset::new(300.0, 188.0));
    frame(&mut ui);
    assert_eq!(ACTION.with(|a| a.borrow().clone()), Some("Copied link".to_string()));
    assert!(!overlay::is_open(), "picking a submenu item closes the menu");
}

#[test]
fn submenu_keyboard_right_enters_and_left_closes() {
    overlay::init();
    pebbles_core::focus::init();
    ACTION.with(|a| *a.borrow_mut() = None);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(600.0, 400.0);
    overlay::set_window_size(600.0, 400.0);
    ui.mount_root(View::new(palette::WHITE, component(sub_root)).into_widget());
    ui.layout(&mut env, win);
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, win);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut env, &mut scene);
    };
    frame(&mut ui);

    tap(&mut ui, Offset::new(20.0, 18.0));
    frame(&mut ui);
    assert!(overlay::is_open());

    // Navigable rows: Profile, Billing, Settings, Share (the label is not a row).
    // ArrowDown ×4 → the sub row; Right enters its child.
    for _ in 0..4 {
        ui.dispatch_key(down());
        frame(&mut ui);
    }
    let right = KeyInput::Move { motion: Motion::Right, extend: false };
    ui.dispatch_key(right.clone());
    frame(&mut ui);
    assert!(overlay::child_is_open(), "Right opens the active row's submenu");

    // Down → the child's first item, Enter runs it and closes everything.
    ui.dispatch_key(down());
    frame(&mut ui);
    ui.dispatch_key(KeyInput::Enter);
    frame(&mut ui);
    assert_eq!(ACTION.with(|a| a.borrow().clone()), Some("Copied link".to_string()));
    assert!(!overlay::is_open(), "picking in the child closes the whole overlay");

    // Left closes the child (reopened via Right) without closing the parent.
    tap(&mut ui, Offset::new(20.0, 18.0));
    frame(&mut ui);
    for _ in 0..4 {
        ui.dispatch_key(down());
        frame(&mut ui);
    }
    ui.dispatch_key(right);
    frame(&mut ui);
    assert!(overlay::child_is_open());
    let left = KeyInput::Move { motion: Motion::Left, extend: false };
    ui.dispatch_key(left);
    frame(&mut ui);
    assert!(!overlay::child_is_open(), "Left closes the child");
    assert!(overlay::is_open(), "the parent menu stays open");
}

#[test]
fn submenu_flips_left_when_the_right_edge_is_full() {
    overlay::init();
    pebbles_core::focus::init();
    animation::reset();
    ACTION.with(|a| *a.borrow_mut() = None);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 400.0); // too narrow for a right-side panel
    overlay::set_window_size(400.0, 400.0);
    ui.mount_root(View::new(palette::WHITE, component(sub_root)).into_widget());
    ui.layout(&mut env, win);
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, win);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut env, &mut scene);
    };
    frame(&mut ui);

    tap(&mut ui, Offset::new(20.0, 18.0));
    frame(&mut ui);
    ui.dispatch_hover(Offset::new(100.0, 188.0));
    frame(&mut ui);
    animation::tick(0.01);
    animation::tick(0.30);
    frame(&mut ui);
    assert!(overlay::child_is_open());

    // The child flipped to the parent's LEFT edge (x ≈ 8..208): its first item
    // (y ≈ 172..204) is tappable there and runs.
    tap(&mut ui, Offset::new(100.0, 188.0));
    frame(&mut ui);
    assert_eq!(ACTION.with(|a| a.borrow().clone()), Some("Copied link".to_string()));
    assert!(!overlay::is_open(), "picking the flipped child closes the menu");
}
