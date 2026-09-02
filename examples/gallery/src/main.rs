//! Pebbles widget gallery — a routed desktop app in the SolidJS-style model,
//! split into one file per screen. Demonstrates: function components, local +
//! global signals, plain-closure events, built-in routing, and props.

mod app;
mod screens;
mod state;
mod styles;
mod ui;

#[cfg(test)]
mod soak;

use pebbles::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Theme::light().make_current();
    state::init(); // create the global route signal before any component runs
    App::new(component(app::app))
        .title("Pebbles — Widget Gallery")
        .size(1180, 820)
        .background(theme().colors.background)
        .run()
}
