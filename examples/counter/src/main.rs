//! Counter — the smallest complete Pebbles app, deliberately structured to show the
//! shape every bigger app follows:
//!
//!   state.rs       → the global state manager (the count + its actions)
//!   components.rs  → small function components (display, controls)
//!   main.rs        → the app shell that composes them
//!
//! Overkill for a counter, yes — but it's the same layout you'd use for a real app.

mod components;
mod state;

use pebbles::prelude::*;

/// The app shell: a centered card holding the two components. Each is mounted with
/// `component(..)` so it re-renders independently when the global count changes.
fn app() -> impl IntoWidget {
    let c = theme().colors;
    center(
        container()
            .decoration(
                BoxDecoration::new()
                    .color(c.card)
                    .border(Border::new(c.border, 1.0))
                    .radius(BorderRadius::all(20.0)),
            )
            .padding(EdgeInsets::symmetric(48.0, 40.0))
            .child(
                column(children![
                    component(components::display),
                    gap_h(28.0),
                    component(components::controls),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_size(MainAxisSize::Min),
            ),
    )
}

#[pebbles::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(component(app))
        .title("Pebbles — Counter")
        .size(460, 480)
        .background(theme().colors.background)
        .run()
}
