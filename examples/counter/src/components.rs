//! The UI, broken into small **function components**.
//!
//! Each is a plain `fn() -> impl IntoWidget`. Mounted with `component(..)`, a
//! component that reads global state re-renders **on its own** when that state
//! changes — so tapping a button re-renders only `display`, not the whole app.

use pebbles::prelude::*;

use crate::state;

/// The big number + a derived sign label. Reads the global count, so it re-renders
/// when the count changes.
pub fn display() -> impl IntoWidget {
    let c = theme().colors;
    let n = state::count().get(); // reading here subscribes THIS component

    // A MEMO (SolidJS `createMemo`): a cached derived value. `sign` recomputes when
    // the count changes — but thanks to the equality firewall, a memo whose value
    // lands the same (e.g. 3 → 5, still "positive") does NOT wake its readers. It's
    // created once (position-stable) and read through the returned signal.
    let sign = create_memo(move || match state::count().get().cmp(&0) {
        std::cmp::Ordering::Less => "negative",
        std::cmp::Ordering::Equal => "zero",
        std::cmp::Ordering::Greater => "positive",
    });

    column(children![
        text("COUNT").size(12.0).weight(600.0).letter_spacing(1.0).color(c.muted_foreground),
        gap_h(6.0),
        text(format!("{n}")).size(84.0).weight(700.0).color(if n < 0 {
            palette::rose::S500
        } else {
            c.foreground
        }),
        gap_h(4.0),
        text(sign.get().to_string()).size(13.0).color(c.muted_foreground),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .main_axis_size(MainAxisSize::Min)
}

/// The controls. Pure UI — they just call the state manager's actions.
pub fn controls() -> impl IntoWidget {
    row(children![
        button("−").variant(ButtonVariant::Outline).size(ButtonSize::Lg).on_pressed(state::decrement),
        gap_w(12.0),
        button("Reset").variant(ButtonVariant::Ghost).on_pressed(state::reset),
        gap_w(12.0),
        button("+").size(ButtonSize::Lg).on_pressed(state::increment),
    ])
    .main_axis_size(MainAxisSize::Min)
}
