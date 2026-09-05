//! The toolbar — the remaining count, the filter tabs (All / Active / Done), and a
//! "Clear done" action. The tabs read + set the store's filter signal.

use pebbles::prelude::*;

use crate::store::{self, Filter};

pub fn toolbar() -> impl IntoWidget {
    let c = theme().colors;
    let remaining = store::remaining();
    let active = store::filter().get();

    row(children![
        text(format!("{remaining} left")).size(13.0).color(c.muted_foreground),
        spacer(),
        tab("All", Filter::All, active),
        tab("Active", Filter::Active, active),
        tab("Done", Filter::Done, active),
        spacer(),
        button("Clear done").variant(ButtonVariant::Ghost).on_pressed(store::clear_completed),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
}

/// One filter tab — a `pressable` region that highlights when it's the active filter.
fn tab(label: &str, filter: Filter, active: Filter) -> impl IntoWidget {
    let c = theme().colors;
    let selected = filter == active;
    let color = if selected { c.primary } else { c.muted_foreground };

    pressable(
        container().padding(EdgeInsets::symmetric(10.0, 5.0)).child(
            text(label.to_string()).size(13.0).weight(if selected { 600.0 } else { 500.0 }).color(color),
        ),
    )
    .radius(7.0)
    .on_tap(move || store::set_filter(filter))
}
