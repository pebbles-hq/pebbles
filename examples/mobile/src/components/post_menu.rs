//! The `⋯` menu on your own posts → a bottom action sheet with **Delete**, which
//! confirms via an alert dialog before removing the post (the common social flow:
//! menu → destructive action → "are you sure").

use pebbles::prelude::*;

use crate::store;

/// Open the post's action sheet (only your own posts get the `⋯`).
pub fn open_post_menu(post_id: u64) {
    sheet(menu(post_id)).side(Side::Bottom).size(150.0).title("Post options").open();
}

fn menu(id: u64) -> impl IntoWidget {
    pressable(
        container().padding(EdgeInsets::symmetric(6.0, 14.0)).child(
            row(children![
                icon(lucide::TRASH_2).size(19.0).color(palette::rose::S500),
                gap_w(12.0),
                text("Delete post").size(15.0).weight(500.0).color(palette::rose::S500),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Center),
        ),
    )
    .radius(10.0)
    .hover_tint(palette::rose::S500)
    .on_tap(move || {
        close_sheet(0); // dismiss the menu, then confirm
        confirm_delete(id);
    })
}

fn confirm_delete(id: u64) {
    alert_dialog("Delete post?")
        .description("This can't be undone.")
        .confirm("Delete")
        .cancel("Cancel")
        .destructive(true)
        .dismissible(true)
        .on_confirm(move || store::delete_post(id))
        .open();
}
