//! The list — maps the store's *visible* (filtered) todos to `item` rows, or shows
//! an empty state. Reading `store::visible()` subscribes this component, so it
//! re-renders whenever the list or the filter changes.

use pebbles::prelude::*;

use super::item::item;
use crate::store;

pub fn list() -> impl IntoWidget {
    let todos = store::visible();

    if todos.is_empty() {
        return empty().into_widget();
    }

    let rows: Vec<AnyWidget> = todos.iter().map(|t| item(t).into_widget()).collect();
    column(rows)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
}

fn empty() -> impl IntoWidget {
    let c = theme().colors;
    container().padding(EdgeInsets::symmetric(0.0, 28.0)).child(
        column(children![
            icon(lucide::CHECK_CHECK).size(28.0).color(c.muted_foreground),
            gap_h(8.0),
            text("Nothing here").size(14.0).color(c.muted_foreground),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_size(MainAxisSize::Min),
    )
}
