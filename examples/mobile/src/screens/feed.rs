//! The home feed — a scrolling list of post cards. Reads `store::feed()`, so a new
//! post (or a like / bookmark) re-renders it.

use pebbles::prelude::*;

use crate::components::post_card;
use crate::store;

pub fn feed() -> impl IntoWidget {
    let posts = store::feed();
    let mut kids: Vec<AnyWidget> = posts.iter().map(|p| post_card(p).into_widget()).collect();
    kids.push(gap_h(16.0).into_widget()); // breathing room under the last card

    scroll_view(
        column(kids).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min),
    )
    .drag_scroll(true)
}
