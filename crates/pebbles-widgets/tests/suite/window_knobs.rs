//! Window-management knobs (checklist 1.7): builder options ride the open request,
//! and the runtime `set_*` helpers enqueue commands the shell drains. Verified through
//! the same queues the shell reads — no OS window needed.

use pebbles_widgets::window::{self, WindowCommand};
use pebbles_widgets::{
    focus_window, minimize_window, set_window_maximized, set_window_position,
    set_window_resizable, set_window_title, text,
};

#[test]
fn open_spec_carries_all_builder_knobs() {
    let id = window::window(text("inspector"))
        .title("Inspector")
        .size(500, 400)
        .min_size(300, 200)
        .max_size(900, 700)
        .position(64, 48)
        .resizable(false)
        .maximized(true)
        .decorations(false)
        .icon(vec![255u8; 4], 1, 1)
        .open();

    let open = window::take_open_requests();
    assert_eq!(open.len(), 1);
    let s = &open[0];
    assert_eq!(s.id, id);
    assert_eq!(s.title, "Inspector");
    assert_eq!((s.width, s.height), (500, 400));
    assert_eq!(s.min_size, Some((300, 200)));
    assert_eq!(s.max_size, Some((900, 700)));
    assert_eq!(s.position, Some((64, 48)));
    assert!(!s.resizable);
    assert!(s.maximized);
    assert!(!s.decorations);
    let icon = s.icon.as_ref().expect("icon carried");
    assert_eq!((icon.width, icon.height, icon.rgba.len()), (1, 1, 4));

    assert!(window::take_open_requests().is_empty(), "drained");
}

#[test]
fn defaults_are_sensible() {
    let _ = window::window(text("x")).open();
    let s = window::take_open_requests().pop().unwrap();
    assert!(s.resizable, "resizable by default");
    assert!(!s.maximized);
    assert!(s.decorations, "decorated by default");
    assert_eq!(s.min_size, None);
    assert_eq!(s.position, None);
}

#[test]
fn runtime_helpers_enqueue_commands() {
    // Nothing pending to start (this test owns the queue on its own thread).
    let _ = window::take_window_commands();

    set_window_title(7, "Renamed");
    set_window_resizable(7, false);
    set_window_maximized(7, true);
    minimize_window(7);
    set_window_position(7, 10, 20);
    window::set_window_size(7, 640, 480);
    focus_window(7);

    let cmds = window::take_window_commands();
    assert_eq!(cmds.len(), 7);
    assert!(matches!(&cmds[0], WindowCommand::SetTitle(7, t) if t == "Renamed"));
    assert!(matches!(cmds[1], WindowCommand::SetResizable(7, false)));
    assert!(matches!(cmds[2], WindowCommand::SetMaximized(7, true)));
    assert!(matches!(cmds[3], WindowCommand::Minimize(7)));
    assert!(matches!(cmds[4], WindowCommand::SetPosition(7, 10, 20)));
    assert!(matches!(cmds[5], WindowCommand::SetSize(7, 640, 480)));
    assert!(matches!(cmds[6], WindowCommand::Focus(7)));
    assert!(window::take_window_commands().is_empty(), "drained");
}
