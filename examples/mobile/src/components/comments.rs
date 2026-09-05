//! The comments sheet for a post — the existing comments (reactively, so a new one
//! appears instantly) plus an add-comment field.

use pebbles::prelude::*;

use super::bits::avatar;
use crate::store;

/// Open the comments sheet for `post_id`.
pub fn open_comments(post_id: u64) {
    sheet(component_props(view, post_id)).side(Side::Bottom).size(460.0).title("Comments").open();
}

// The `&u64` is required by `component_props(fn(&P), props)`, not a choice.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn view(post_id: &u64) -> AnyWidget {
    let id = *post_id;
    let draft = create_signal(String::new());
    let c = theme().colors;

    // Reading the post subscribes this sheet — adding a comment re-renders it.
    let comments = store::post(id).map(|p| p.comments).unwrap_or_default();
    let rows: Vec<AnyWidget> = if comments.is_empty() {
        vec![
            container()
                .padding(EdgeInsets::symmetric(0.0, 24.0))
                .child(text("No comments yet — say something!").size(13.5).color(c.muted_foreground))
                .into_widget(),
        ]
    } else {
        comments.iter().map(|cm| comment_row(cm).into_widget()).collect()
    };

    let send = move || {
        store::add_comment(id, &draft.peek());
        draft.set(String::new());
    };

    column(children![
        Expanded::new(scroll_view(
            column(rows).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min),
        )),
        gap_h(8.0),
        row(children![
            Expanded::new(text_field().bind(draft).placeholder("Add a comment").on_submit(move |_| send())),
            gap_w(8.0),
            icon_button(lucide::SEND).on_pressed(send),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .main_axis_size(MainAxisSize::Max)
    .into_widget()
}

fn comment_row(cm: &store::Comment) -> impl IntoWidget {
    let c = theme().colors;
    let author = store::user(cm.author);
    container().padding(EdgeInsets::symmetric(0.0, 8.0)).child(
        row(children![
            avatar(&author.avatar, 32.0),
            gap_w(10.0),
            Expanded::new(
                column(children![
                    text(author.name.clone()).size(13.5).semibold().color(c.foreground),
                    gap_h(2.0),
                    text(cm.text.clone()).size(13.5).line_height(1.4).color(c.foreground),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min),
            ),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start),
    )
}
