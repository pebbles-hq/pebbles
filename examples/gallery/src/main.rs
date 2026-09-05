//! Pebbles widget gallery — a routed desktop app in the SolidJS-style model,
//! split into one file per screen. Demonstrates: function components, local +
//! global signals, plain-closure events, built-in routing, and props.

mod app;
mod capture;
mod screens;
mod state;
mod styles;
mod ui;

#[cfg(test)]
mod soak;

use pebbles::prelude::*;

#[pebbles::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Theme::light().make_current();
    state::init(); // create the global route signal before any component runs
    // GALLERY_CAPTURE=<dir>: render the two-window IPC demo headlessly to raw
    // RGBA frames (the pre-competition demo capture) instead of opening a window.
    if let Ok(dir) = std::env::var("GALLERY_CAPTURE") {
        return capture::run(&dir);
    }
    App::new(component(app::app))
        .title("Pebbles — Widget Gallery")
        .size(1180, 820)
        .background(theme().colors.background)
        // B3: a native OS menu bar. Takes effect on macOS/Windows when built with
        // `--features pebbles/native-menus`; a no-op otherwise (the in-window
        // menubar(..) demo stays the cross-platform form). Clicks print to stderr so
        // the callback routing is observable.
        .menu(menu_bar([
            menu(
                "File",
                [
                    menu_item("New").shortcut("Mod+N").on_select(|| eprintln!("File → New")).into(),
                    menu_item("Open…").shortcut("Mod+O").on_select(|| eprintln!("File → Open")).into(),
                    menu_separator(),
                    menu_item("Quit").shortcut("Mod+Q").on_select(|| std::process::exit(0)).into(),
                ],
            ),
            menu(
                "Edit",
                [
                    menu_item("Copy").shortcut("Mod+C").on_select(|| eprintln!("Edit → Copy")).into(),
                    menu_item("Paste").shortcut("Mod+V").on_select(|| eprintln!("Edit → Paste")).into(),
                ],
            ),
            menu(
                "View",
                [menu_check("Notifications", true, |on| eprintln!("View → Notifications: {on}")).into()],
            ),
        ]))
        .run()
}
