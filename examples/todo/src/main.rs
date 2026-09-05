//! Todo — a small app organized the way a real one is:
//!
//!   store.rs        → the state manager (the todos + filter, and the actions on them)
//!   components/     → one file per component (input, item, list, toolbar)
//!   main.rs         → the app shell that lays them out
//!
//! The point: state lives in ONE place (`store`), the UI is small composable pieces,
//! and each piece re-renders on its own when the state it reads changes.

mod components;
mod store;

use pebbles::prelude::*;

fn app() -> impl IntoWidget {
    let c = theme().colors;
    center(
        container()
            .width(480.0)
            .decoration(
                BoxDecoration::new()
                    .color(c.card)
                    .border(Border::new(c.border, 1.0))
                    .radius(BorderRadius::all(18.0)),
            )
            .padding(EdgeInsets::all(24.0))
            .child(
                column(children![
                    text("Todo").size(24.0).weight(700.0).color(c.foreground),
                    gap_h(4.0),
                    text("A tiny app, organized like a real one.").size(13.5).color(c.muted_foreground),
                    gap_h(20.0),
                    // Each piece is its own component: `input` owns local draft state,
                    // `list` + `toolbar` read the store and re-render independently.
                    component(components::input),
                    gap_h(18.0),
                    component(components::list),
                    gap_h(16.0),
                    container().color(c.border).height(1.0),
                    gap_h(12.0),
                    component(components::toolbar),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_size(MainAxisSize::Min),
            ),
    )
}

#[pebbles::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(component(app))
        .title("Pebbles — Todo")
        .size(560, 640)
        .background(theme().colors.background)
        .run()
}
