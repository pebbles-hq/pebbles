//! The home feed — an **infinite-scrolling** list of post cards.
//!
//! The posts stream in from a real API (see `store`/`net`). On mount we kick off the
//! first page; as you scroll near the bottom, `on_scroll` asks the store for the next
//! page. Each fetch runs off the UI thread and appends its results by writing a
//! signal, so this function just re-renders whatever's loaded so far — the async
//! machinery is entirely in the store.

use pebbles::prelude::*;

use crate::components::post_card;
use crate::store::{self, LoadState};

pub fn feed() -> impl IntoWidget {
    component(feed_view)
}

fn feed_view() -> impl IntoWidget {
    // Start loading the moment the feed first mounts (self-guards against re-mounts).
    create_effect(store::ensure_feed_started);

    let posts = store::feed(); // subscribes: a new page (or a like) re-renders us
    let state = store::feed_state();

    let mut kids: Vec<AnyWidget> = posts.iter().map(|p| post_card(p).into_widget()).collect();
    kids.push(footer(state));

    scroll_view(
        column(kids).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min),
    )
    .on_scroll(|n| {
        // Within ~900px of the end → pull the next page (the store ignores this while
        // a fetch is in flight or once everything has loaded).
        if n.metrics.extent_after() < 900.0 {
            store::load_feed_more();
        }
    })
    .drag_scroll(true)
}

/// The list footer: a spinner while paging, a retry on error, or an end marker.
fn footer(state: LoadState) -> AnyWidget {
    let c = theme().colors;

    let content: AnyWidget = match state {
        LoadState::Loading | LoadState::Idle => row(children![
            spinner(18.0).color(c.muted_foreground),
            gap_w(10.0),
            text("Loading more…").size(13.0).color(c.muted_foreground),
        ])
        .main_axis_alignment(MainAxisAlignment::Center)
        .main_axis_size(MainAxisSize::Min)
        .into_widget(),

        LoadState::Error => pressable(text("Couldn't load. Tap to retry").size(13.0).color(c.primary))
            .radius(8.0)
            .on_tap(store::load_feed_more)
            .into_widget(),

        LoadState::Done => text("You're all caught up ✦").size(13.0).color(c.muted_foreground).into_widget(),

        LoadState::Loaded => gap_h(4.0).into_widget(),
    };

    container()
        .padding(EdgeInsets::symmetric(16.0, 22.0))
        .child(row(children![spacer(), content, spacer()]).cross_axis_alignment(CrossAxisAlignment::Center))
        .into_widget()
}
