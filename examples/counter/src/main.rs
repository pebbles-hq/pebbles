//! Counter — the smallest complete Pebbles app, deliberately structured to show the
//! shape every bigger app follows:
//!
//!   state.rs       → the global state manager (the count + its actions)
//!   components.rs  → small function components (display, controls)
//!   main.rs        → the app shell that composes them
//!
//! Overkill for a counter, yes — but it's the same layout you'd use for a real app.
//!
//! It also demos the reactivity toolkit (Solid-inspired, but idiomatic Rust — plain
//! functions + `Copy` signal handles, no proxies):
//!
//!   Solid `createSignal`  → `create_signal`        (local state — see input fields)
//!   Solid app-scope signal→ `create_root_signal`   (global state — see state.rs)
//!   Solid `createMemo`     → `create_memo`          (cached derived — see components.rs)
//!   Solid `createEffect`   → `create_effect`        (side effects — see `app` below)
//!   Solid `onCleanup`      → `create_cleanup`       (runs on unmount)
//!   Solid `createStore`    → `create_store`         (an object-shaped store)
//!   Solid `untrack`        → `untrack`              (read without subscribing)

mod components;
mod state;

use pebbles::prelude::*;

/// The app shell: a centered card holding the two components. Each is mounted with
/// `component(..)` so it re-renders independently when the global count changes.
fn app() -> impl IntoWidget {
    // An EFFECT (SolidJS `createEffect`): a side effect that re-runs whenever a signal
    // it reads changes. This logs to the terminal on every count change — the textbook
    // "react to state" example (watch your terminal as you click). It's created ONCE
    // and re-runs on the count, not on every render.
    create_effect(|| println!("[effect] count is now {}", state::count().get()));

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
