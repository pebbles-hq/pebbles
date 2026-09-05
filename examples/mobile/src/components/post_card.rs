//! A single feed post — author, text, optional media, and the like / comment /
//! bookmark action bar. Tapping the post body opens its full detail view; every
//! action calls a store action, and the card re-renders when the post's state changes.

use pebbles::prelude::*;

use super::bits::avatar;
use super::post_menu::open_post_menu;
use crate::store::{self, Post};

pub fn post_card(post: &Post) -> impl IntoWidget {
    let c = theme().colors;
    let id = post.id;
    let author = store::user(post.author);

    // Subtitle: "@handle · 2h", or just "@handle" for API posts that carry no time.
    let subtitle = if post.time.is_empty() {
        format!("@{}", author.handle)
    } else {
        format!("@{} · {}", author.handle, post.time)
    };

    // Header: avatar, name/handle, follow (for others) or a ⋯ menu (for your own).
    let header = row(children![
        avatar(&author.avatar, 42.0),
        gap_w(10.0),
        column(children![
            text(author.name.clone()).size(14.5).semibold().color(c.foreground),
            text(subtitle).size(12.5).color(c.muted_foreground),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min),
        spacer(),
        follow_or_menu(&author, id),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center);

    // Body text (skipped when empty).
    let mut body: Vec<AnyWidget> = vec![header.into_widget()];
    if !post.text.is_empty() {
        body.push(gap_h(10.0).into_widget());
        body.push(
            text(post.text.clone())
                .size(14.5)
                .line_height(1.45)
                .max_lines(6)
                .ellipsis()
                .color(c.foreground)
                .into_widget(),
        );
    }

    // Media (network image), rounded + clipped.
    if let Some(url) = &post.media {
        body.push(gap_h(12.0).into_widget());
        body.push(
            container()
                .decoration(BoxDecoration::new().color(c.secondary).radius(BorderRadius::all(14.0)))
                .clip()
                .height(220.0)
                .child(ImageView::network(url.clone()).fit(ImageFit::Cover))
                .into_widget(),
        );
    }

    // Tapping the header/text/media opens the post; the action row below stays its
    // own set of buttons (they're inner listeners, so they win their own taps).
    let tappable = GestureDetector::new(
        column(body).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min),
    )
    .on_tap(move || store::open_post(id));

    container()
        .decoration(
            BoxDecoration::new()
                .color(c.card)
                .border(Border::new(c.border, 1.0))
                .radius(BorderRadius::all(16.0)),
        )
        .padding(EdgeInsets::all(14.0))
        .margin(EdgeInsets::only(14.0, 14.0, 14.0, 0.0))
        .child(
            column(children![tappable, gap_h(12.0), actions(post)])
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_size(MainAxisSize::Min),
        )
}

fn follow_or_menu(author: &store::User, post_id: u64) -> AnyWidget {
    if author.id == store::ME {
        // Your own post: a ⋯ menu (delete, etc.).
        icon_button(lucide::ELLIPSIS)
            .variant(ButtonVariant::Ghost)
            .on_pressed(move || open_post_menu(post_id))
            .into_widget()
    } else {
        let id = author.id;
        super::bits::pill(if author.i_follow { "Following" } else { "Follow" }, !author.i_follow, move || {
            store::toggle_follow(id)
        })
        .into_widget()
    }
}

fn actions(post: &Post) -> impl IntoWidget {
    let c = theme().colors;
    let id = post.id;
    let heart = if post.liked { palette::rose::S500 } else { c.muted_foreground };

    row(children![
        action(lucide::HEART, heart, Some(post.likes), move || store::toggle_like(id)),
        gap_w(18.0),
        // The comment icon opens the full post + its thread.
        action(lucide::MESSAGE_CIRCLE, c.muted_foreground, post.comment_count, move || {
            store::open_post(id)
        }),
        spacer(),
        action(
            lucide::BOOKMARK,
            if post.bookmarked { c.primary } else { c.muted_foreground },
            None,
            move || store::toggle_bookmark(id),
        ),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
}

/// One tappable action — an icon with an optional count.
fn action(ic: IconData, color: Color, count: Option<u32>, on_tap: impl Fn() + 'static) -> impl IntoWidget {
    let c = theme().colors;
    let mut kids: Vec<AnyWidget> = vec![icon(ic).size(19.0).color(color).into_widget()];
    if let Some(n) = count {
        kids.push(gap_w(6.0).into_widget());
        kids.push(text(format!("{n}")).size(13.0).color(c.muted_foreground).into_widget());
    }
    pressable(row(kids).cross_axis_alignment(CrossAxisAlignment::Center)).radius(8.0).on_tap(on_tap)
}
