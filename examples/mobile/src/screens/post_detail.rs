//! The **post detail** screen — a full-screen takeover you reach by tapping a post.
//!
//! It shows the whole post, then its comment thread: real comments loaded from the
//! API (or your local ones), each likeable and replyable. Comments load *on open* via
//! a mount effect; you can add a comment or reply to one, and it appears instantly.
//! This is where the app's async + reactive state really shows off.

use pebbles::prelude::*;

use crate::components::bits::avatar;
use crate::components::post_menu::open_post_menu;
use crate::store::{self, Comment, LoadState, Post};

/// Open the detail view for `post_id`.
pub fn post_detail(post_id: u64) -> AnyWidget {
    component_props(view, post_id).into_widget()
}

// `&u64` is required by `component_props(fn(&P), props)`, not a choice.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn view(post_id: &u64) -> AnyWidget {
    let pid = *post_id;
    let c = theme().colors;

    // Load this post's comments the first time the screen mounts (self-guards).
    create_effect(move || store::load_comments(pid));

    // Local composer state: the draft, and which comment (if any) we're replying to.
    let draft = create_signal(String::new());
    let replying_to = create_signal::<Option<u64>>(None);

    let Some(post) = store::post(pid) else {
        return not_found();
    };
    let (state, comments) = store::comment_thread(pid);

    // Scrollable body: the post, then the comment thread under it.
    let mut body: Vec<AnyWidget> =
        vec![hero(pid, &post).into_widget(), divider().into_widget(), thread_header(&post).into_widget()];
    body.push(thread_body(pid, state, &comments, replying_to).into_widget());
    body.push(gap_h(12.0).into_widget());

    let scroller = Expanded::new(
        scroll_view(
            column(body).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min),
        )
        .drag_scroll(true),
    );

    scaffold(
        column(children![scroller, composer(pid, draft, replying_to, &comments)])
            .cross_axis_alignment(CrossAxisAlignment::Stretch),
    )
    .top(
        top_panel("Post").leading(
            icon_button(lucide::ARROW_LEFT).variant(ButtonVariant::Ghost).on_pressed(store::close_post),
        ),
    )
    .background(c.background)
    .into_widget()
}

// ---------------------------------------------------------------------------
// The post itself
// ---------------------------------------------------------------------------

fn hero(pid: u64, post: &Post) -> impl IntoWidget {
    let c = theme().colors;
    let author = store::user(post.author);
    let subtitle = if post.time.is_empty() {
        format!("@{}", author.handle)
    } else {
        format!("@{} · {}", author.handle, post.time)
    };

    let header = row(children![
        avatar(&author.avatar, 46.0),
        gap_w(11.0),
        column(children![
            text(author.name.clone()).size(15.5).semibold().color(c.foreground),
            text(subtitle).size(13.0).color(c.muted_foreground),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min),
        spacer(),
        menu_or_gap(post.author, pid),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center);

    let mut kids: Vec<AnyWidget> = vec![header.into_widget()];
    if !post.text.is_empty() {
        kids.push(gap_h(12.0).into_widget());
        kids.push(text(post.text.clone()).size(15.5).line_height(1.5).color(c.foreground).into_widget());
    }
    if let Some(url) = &post.media {
        kids.push(gap_h(14.0).into_widget());
        kids.push(
            container()
                .decoration(BoxDecoration::new().color(c.secondary).radius(BorderRadius::all(16.0)))
                .clip()
                .height(260.0)
                .child(ImageView::network(url.clone()).fit(ImageFit::Cover))
                .into_widget(),
        );
    }
    kids.push(gap_h(14.0).into_widget());
    kids.push(hero_actions(post).into_widget());

    container().padding(EdgeInsets::all(16.0)).child(
        column(kids).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min),
    )
}

/// The post's own like / bookmark bar (comment count is shown in the thread header).
fn hero_actions(post: &Post) -> impl IntoWidget {
    let c = theme().colors;
    let id = post.id;
    let heart = if post.liked { palette::rose::S500 } else { c.muted_foreground };
    let bookmark = if post.bookmarked { c.primary } else { c.muted_foreground };

    row(children![
        stat_button(lucide::HEART, heart, Some(post.likes), move || store::toggle_like(id)),
        gap_w(20.0),
        stat_button(lucide::MESSAGE_CIRCLE, c.muted_foreground, post.comment_count, || {}),
        spacer(),
        stat_button(lucide::BOOKMARK, bookmark, None, move || store::toggle_bookmark(id)),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
}

fn stat_button(
    ic: IconData,
    color: Color,
    count: Option<u32>,
    on_tap: impl Fn() + 'static,
) -> impl IntoWidget {
    let c = theme().colors;
    let mut kids: Vec<AnyWidget> = vec![icon(ic).size(20.0).color(color).into_widget()];
    if let Some(n) = count {
        kids.push(gap_w(7.0).into_widget());
        kids.push(text(format!("{n}")).size(13.5).color(c.muted_foreground).into_widget());
    }
    pressable(row(kids).cross_axis_alignment(CrossAxisAlignment::Center)).radius(8.0).on_tap(on_tap)
}

fn menu_or_gap(author: u64, pid: u64) -> AnyWidget {
    if author == store::ME {
        icon_button(lucide::ELLIPSIS)
            .variant(ButtonVariant::Ghost)
            .on_pressed(move || open_post_menu(pid))
            .into_widget()
    } else {
        gap_w(0.0).into_widget()
    }
}

// ---------------------------------------------------------------------------
// The comment thread
// ---------------------------------------------------------------------------

fn thread_header(post: &Post) -> impl IntoWidget {
    let c = theme().colors;
    let label = match post.comment_count {
        Some(0) | None => "Comments".to_string(),
        Some(1) => "1 comment".to_string(),
        Some(n) => format!("{n} comments"),
    };
    container()
        .padding(EdgeInsets::only(16.0, 14.0, 16.0, 6.0))
        .child(text(label).size(15.0).semibold().color(c.foreground))
}

/// The thread contents — a spinner, an error/retry, an empty note, or the comments.
fn thread_body(
    pid: u64,
    state: LoadState,
    comments: &[Comment],
    replying_to: Signal<Option<u64>>,
) -> AnyWidget {
    let c = theme().colors;

    match state {
        LoadState::Loading | LoadState::Idle => padded(
            row(children![
                spinner(18.0).color(c.muted_foreground),
                gap_w(10.0),
                text("Loading comments…").size(13.5).color(c.muted_foreground),
            ])
            .main_axis_size(MainAxisSize::Min),
        ),
        LoadState::Error => padded(
            pressable(text("Couldn't load comments. Tap to retry").size(13.5).color(c.primary))
                .radius(8.0)
                .on_tap(move || store::load_comments(pid)),
        ),
        LoadState::Loaded | LoadState::Done if comments.is_empty() => {
            padded(text("No comments yet — start the conversation.").size(13.5).color(c.muted_foreground))
        }
        LoadState::Loaded | LoadState::Done => {
            let rows: Vec<AnyWidget> =
                comments.iter().map(|cm| comment_view(pid, cm, replying_to).into_widget()).collect();
            column(rows)
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_size(MainAxisSize::Min)
                .into_widget()
        }
    }
}

/// A padded single-line block, centered horizontally (for spinner/empty/error notes).
fn padded(child: impl IntoWidget) -> AnyWidget {
    container()
        .padding(EdgeInsets::symmetric(16.0, 18.0))
        .child(row(children![spacer(), child.into_widget(), spacer()]))
        .into_widget()
}

/// One top-level comment: author + text + like/reply actions, then its replies.
fn comment_view(pid: u64, cm: &Comment, replying_to: Signal<Option<u64>>) -> impl IntoWidget {
    let c = theme().colors;
    let author = store::user(cm.author);
    let cid = cm.id;

    let actions = row(children![
        like_pill(pid, cm),
        gap_w(16.0),
        pressable(text("Reply").size(12.5).weight(600.0).color(c.muted_foreground))
            .radius(6.0)
            .on_tap(move || replying_to.set(Some(cid))),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .main_axis_size(MainAxisSize::Min);

    let mut column_kids: Vec<AnyWidget> = vec![
        comment_body(&author.name, &cm.text).into_widget(),
        gap_h(6.0).into_widget(),
        actions.into_widget(),
    ];
    // Replies, indented beneath the parent.
    if !cm.replies.is_empty() {
        let replies: Vec<AnyWidget> = cm
            .replies
            .iter()
            .map(|r| {
                let author = store::user(r.author);
                container()
                    .padding(EdgeInsets::only(0.0, 12.0, 0.0, 0.0))
                    .child(
                        row(children![
                            avatar(&author.avatar, 26.0),
                            gap_w(9.0),
                            Expanded::new(
                                column(children![
                                    comment_body(&author.name, &r.text),
                                    gap_h(5.0),
                                    like_pill(pid, r),
                                ])
                                .cross_axis_alignment(CrossAxisAlignment::Start)
                                .main_axis_size(MainAxisSize::Min),
                            ),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Start),
                    )
                    .into_widget()
            })
            .collect();
        column_kids.push(
            container()
                .padding(EdgeInsets::only(10.0, 10.0, 0.0, 0.0))
                .child(
                    column(replies)
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .main_axis_size(MainAxisSize::Min),
                )
                .into_widget(),
        );
    }

    container().padding(EdgeInsets::symmetric(16.0, 10.0)).child(
        row(children![
            avatar(&author.avatar, 34.0),
            gap_w(10.0),
            Expanded::new(
                column(column_kids)
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
            ),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start),
    )
}

/// Author name over the comment text.
fn comment_body(name: &str, body: &str) -> impl IntoWidget {
    let c = theme().colors;
    column(children![
        text(name.to_string()).size(13.5).semibold().color(c.foreground),
        gap_h(2.0),
        text(body.to_string()).size(13.5).line_height(1.4).color(c.foreground),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_size(MainAxisSize::Min)
}

/// A small like control for a comment/reply — a heart + its count.
fn like_pill(pid: u64, cm: &Comment) -> impl IntoWidget {
    let c = theme().colors;
    let cid = cm.id;
    let color = if cm.liked { palette::rose::S500 } else { c.muted_foreground };
    let count = cm.likes;

    let mut kids: Vec<AnyWidget> = vec![icon(lucide::HEART).size(15.0).color(color).into_widget()];
    if count > 0 {
        kids.push(gap_w(5.0).into_widget());
        kids.push(text(format!("{count}")).size(12.5).color(c.muted_foreground).into_widget());
    }
    pressable(row(kids).cross_axis_alignment(CrossAxisAlignment::Center))
        .radius(6.0)
        .on_tap(move || store::toggle_comment_like(pid, cid))
}

// ---------------------------------------------------------------------------
// The composer — add a comment, or reply to one
// ---------------------------------------------------------------------------

fn composer(
    pid: u64,
    draft: Signal<String>,
    replying_to: Signal<Option<u64>>,
    comments: &[Comment],
) -> impl IntoWidget {
    let c = theme().colors;

    let send = move || {
        let text = draft.peek();
        match replying_to.peek() {
            Some(parent) => store::add_reply(pid, parent, &text),
            None => store::add_comment(pid, &text),
        }
        draft.set(String::new());
        replying_to.set(None);
    };

    // A "replying to @handle" chip when a reply target is set.
    let chip: AnyWidget = match replying_to.get() {
        Some(cid) => {
            let handle = comments
                .iter()
                .find(|cm| cm.id == cid)
                .map(|cm| store::user(cm.author).handle)
                .unwrap_or_default();
            container()
                .color(c.secondary)
                .padding(EdgeInsets::symmetric(14.0, 8.0))
                .child(
                    row(children![
                        text(format!("Replying to @{handle}")).size(12.5).color(c.muted_foreground),
                        spacer(),
                        pressable(icon(lucide::X).size(15.0).color(c.muted_foreground))
                            .radius(6.0)
                            .on_tap(move || replying_to.set(None)),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Center),
                )
                .into_widget()
        }
        None => gap_h(0.0).into_widget(),
    };

    let placeholder = if replying_to.get().is_some() { "Write a reply…" } else { "Add a comment" };

    let input = container()
        .decoration(BoxDecoration::new().color(c.background).border(Border::new(c.border, 1.0)))
        .padding(EdgeInsets::all(10.0))
        .child(
            row(children![
                Expanded::new(text_field().bind(draft).placeholder(placeholder).on_submit(move |_| send())),
                gap_w(8.0),
                icon_button(lucide::SEND).on_pressed(send),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Center),
        );

    column(children![chip, input])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
}

// ---------------------------------------------------------------------------
// bits
// ---------------------------------------------------------------------------

fn divider() -> impl IntoWidget {
    container().color(theme().colors.border).height(1.0)
}

fn not_found() -> AnyWidget {
    let c = theme().colors;
    scaffold(
        container()
            .padding(EdgeInsets::all(24.0))
            .child(text("This post is no longer available.").size(14.0).color(c.muted_foreground)),
    )
    .top(
        top_panel("Post").leading(
            icon_button(lucide::ARROW_LEFT).variant(ButtonVariant::Ghost).on_pressed(store::close_post),
        ),
    )
    .into_widget()
}
