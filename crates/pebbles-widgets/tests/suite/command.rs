//! Command list + palette, and the shared SI-4 list-keyboard-navigation model:
//! typing filters the rows, Arrow keys move the highlighted row, Enter runs it.
//! Driven headlessly through the focus/editor channel (autofocus → dispatch_key).

use std::cell::RefCell;

use pebbles_core::{IntoWidget, KeyInput, Motion, Ui, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_testing::{draw_frame as frame};
use pebbles_widgets::{
    OverlayHost, View, combobox, command, command_group, command_item, dialog,
};

thread_local! {
    static PICKED: RefCell<Option<String>> = const { RefCell::new(None) };
    static COMBO: RefCell<Option<usize>> = const { RefCell::new(None) };
}

fn down() -> KeyInput {
    KeyInput::Move { motion: Motion::Down, extend: false }
}

fn cmd_root() -> impl IntoWidget {
    OverlayHost::wrap(
        command([
            command_group(
                "Suggestions",
                [
                    command_item("New File").on_select(|| PICKED.with(|p| *p.borrow_mut() = Some("New File".into()))),
                    command_item("New Folder").on_select(|| PICKED.with(|p| *p.borrow_mut() = Some("New Folder".into()))),
                    command_item("Open Recent").on_select(|| PICKED.with(|p| *p.borrow_mut() = Some("Open Recent".into()))),
                ],
            ),
            command_group(
                "Settings",
                [
                    command_item("Toggle Theme").on_select(|| PICKED.with(|p| *p.borrow_mut() = Some("Toggle Theme".into()))),
                ],
            ),
        ])
        .width(420.0),
    )
}

#[test]
fn command_filters_then_arrow_enter_picks() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    PICKED.with(|p| *p.borrow_mut() = None);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(500.0, 420.0);
    ui.mount_root(View::new(palette::WHITE, component(cmd_root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win); // autofocus grabs the search editor

    // Filter to the two "New …" items (spanning is within one group here).
    ui.dispatch_key(KeyInput::Insert("New".to_string()));
    frame(&mut ui, &mut env, win);

    // ArrowDown → row 0 ("New File"); ArrowDown → row 1 ("New Folder"); Enter picks it.
    ui.dispatch_key(down());
    frame(&mut ui, &mut env, win);
    ui.dispatch_key(down());
    frame(&mut ui, &mut env, win);
    ui.dispatch_key(KeyInput::Enter);
    frame(&mut ui, &mut env, win);

    assert_eq!(
        PICKED.with(|p| p.borrow().clone()),
        Some("New Folder".to_string()),
        "arrow-down twice then Enter picks the second filtered row"
    );
}

fn combo_root() -> impl IntoWidget {
    OverlayHost::wrap(
        combobox(["Apple", "Banana", "Cherry", "Date", "Elderberry"])
            .width(220.0)
            .on_changed(|i, _| COMBO.with(|c| *c.borrow_mut() = Some(i))),
    )
}

#[test]
fn combobox_arrow_down_twice_then_enter_picks_index_one() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    COMBO.with(|c| *c.borrow_mut() = None);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 500.0);
    ui.mount_root(View::new(palette::WHITE, component(combo_root)).into_widget());
    ui.layout(&mut env, win);
    frame(&mut ui, &mut env, win);

    // Open the menu by tapping the trigger (top-left, packed to the start of the column).
    let trigger = Offset::new(20.0, 18.0);
    ui.dispatch_pointer_down(trigger);
    ui.dispatch_tap(trigger);
    ui.dispatch_pointer_up(trigger);
    frame(&mut ui, &mut env, win); // the search field autofocuses

    // ArrowDown ×2 → active row 1 ("Banana"), Enter picks it (unfiltered list).
    ui.dispatch_key(down());
    frame(&mut ui, &mut env, win);
    ui.dispatch_key(down());
    frame(&mut ui, &mut env, win);
    ui.dispatch_key(KeyInput::Enter);
    frame(&mut ui, &mut env, win);

    assert_eq!(COMBO.with(|c| *c.borrow()), Some(1), "arrow-down ×2 + Enter picks index 1 (Banana)");
}

#[test]
fn command_palette_opens_a_modal() {
    dialog::init();
    pebbles_core::focus::init();
    pebbles_widgets::command_palette([command_group("Go", [command_item("Home")])]).open();
    assert!(dialog::is_open(), "the palette opens a modal");
    dialog::dismiss_top();
    assert!(!dialog::is_open(), "dismissible by default");
}
