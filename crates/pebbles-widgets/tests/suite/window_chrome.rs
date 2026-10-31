//! Client-side window decorations (`window_chrome`): the resize borders show the CSS
//! resize cursors and hand a press to the window manager, they disappear while the
//! window is maximised or full screen, and the title-bar drag area moves the window on a
//! drag, toggles maximise on a double-click and opens the window menu on a right-click.
//! Every call is asserted through the window command queue (no OS window).

use pebbles_core::widget::AnyWidget;
use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::{Cursor, TextEnv};
use pebbles_widgets::window::{
    MAIN_WINDOW, ResizeEdge, WindowCommand, WindowState, set_window_state, take_window_commands,
};
use pebbles_widgets::window_chrome::{window_drag_area, window_resize_frame};
use pebbles_widgets::{Container, View};

const W: f64 = 800.0;
const H: f64 = 400.0;

fn mount(root: fn() -> AnyWidget) -> (Ui, TextEnv) {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let mut ui = Ui::new();
    let env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    (ui, env)
}

fn pump(ui: &mut Ui, env: &mut TextEnv) {
    for _ in 0..2 {
        ui.rebuild_if_dirty();
        ui.layout(env, Size::new(W, H));
        ui.publish_bounds();
    }
}

fn framed() -> AnyWidget {
    window_resize_frame(MAIN_WINDOW, 6.0, Container::new().color(palette::WHITE)).into_widget()
}

#[test]
fn resize_borders_cursors_and_hand_off_unless_maximised() {
    set_window_state(MAIN_WINDOW, WindowState { focused: true, ..Default::default() });
    let (mut ui, mut env) = mount(framed);
    pump(&mut ui, &mut env);
    let probes = [
        (Offset::new(W / 2.0, 2.0), ResizeEdge::North, Cursor::NsResize),
        (Offset::new(W / 2.0, H - 2.0), ResizeEdge::South, Cursor::NsResize),
        (Offset::new(2.0, H / 2.0), ResizeEdge::West, Cursor::EwResize),
        (Offset::new(W - 2.0, H / 2.0), ResizeEdge::East, Cursor::EwResize),
        (Offset::new(2.0, 2.0), ResizeEdge::NorthWest, Cursor::NwseResize),
        (Offset::new(W - 2.0, H - 2.0), ResizeEdge::SouthEast, Cursor::NwseResize),
        (Offset::new(W - 2.0, 2.0), ResizeEdge::NorthEast, Cursor::NeswResize),
        (Offset::new(2.0, H - 2.0), ResizeEdge::SouthWest, Cursor::NeswResize),
    ];
    take_window_commands();
    for (at, edge, cursor) in probes {
        assert_eq!(ui.cursor_at(at), Some(cursor), "{edge:?}");
        ui.dispatch_pointer_down(at);
        ui.dispatch_pointer_up(at);
        assert_eq!(take_window_commands(), vec![WindowCommand::DragResize(MAIN_WINDOW, edge)]);
    }
    // The middle is the app's own.
    assert_eq!(ui.cursor_at(Offset::new(W / 2.0, H / 2.0)), None);
    ui.dispatch_pointer_down(Offset::new(W / 2.0, H / 2.0));
    assert!(take_window_commands().is_empty());

    // Maximised (or full screen): no borders.
    for st in [
        WindowState { maximized: true, focused: true, ..Default::default() },
        WindowState { fullscreen: true, focused: true, ..Default::default() },
    ] {
        set_window_state(MAIN_WINDOW, st);
        pump(&mut ui, &mut env);
        for (at, ..) in probes {
            assert_eq!(ui.cursor_at(at), None, "{st:?}");
            ui.dispatch_pointer_down(at);
        }
        assert!(take_window_commands().is_empty());
    }
    set_window_state(MAIN_WINDOW, WindowState { focused: true, ..Default::default() });
    pump(&mut ui, &mut env);
    assert_eq!(ui.cursor_at(Offset::new(2.0, 2.0)), Some(Cursor::NwseResize), "back when restored");
}

fn bar() -> AnyWidget {
    window_drag_area(MAIN_WINDOW, Container::new().color(palette::WHITE))
}

#[test]
fn drag_area_moves_toggles_and_opens_the_window_menu() {
    let (mut ui, mut env) = mount(bar);
    pump(&mut ui, &mut env);
    take_window_commands();
    let at = Offset::new(300.0, 20.0);
    // A press alone is not a move (a click stays a click)...
    let mut pan = pebbles_core::pan::PanGesture::default();
    pan.press(ui.pan_target_at(at), at, pebbles_core::pan::MOUSE_PAN_SLOP);
    pan.moved(&mut ui, Offset::new(303.0, 20.0));
    assert!(take_window_commands().is_empty());
    // ...past the slop the window manager takes the move.
    pan.moved(&mut ui, Offset::new(330.0, 25.0));
    assert_eq!(take_window_commands(), vec![WindowCommand::DragMove(MAIN_WINDOW)]);
    pan.release(&mut ui, Offset::new(330.0, 25.0));

    assert!(ui.dispatch_double_tap(at));
    assert_eq!(take_window_commands(), vec![WindowCommand::ToggleMaximized(MAIN_WINDOW)]);

    assert!(ui.dispatch_secondary_tap(at));
    assert_eq!(take_window_commands(), vec![WindowCommand::ShowWindowMenu(MAIN_WINDOW, 300.0, 20.0)]);
}
