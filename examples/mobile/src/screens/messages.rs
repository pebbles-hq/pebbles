//! Messaging — a full-screen takeover (opened from the top-bar button) with two
//! levels: a conversation **list** and a **thread**. Which one shows is a global
//! `MsgView` signal in the store, so the top bar's back button just walks it.

use pebbles::prelude::*;

use crate::components::bits::avatar;
use crate::store::{self, Message, MsgView};

/// Routes the messaging surface to the list or a thread.
pub fn messages() -> AnyWidget {
    match store::messages_view() {
        MsgView::List => component(list).into_widget(),
        MsgView::Thread(id) => component_props(thread, id).into_widget(),
        MsgView::Closed => gap_h(0.0).into_widget(),
    }
}

// ---------------------------------------------------------------------------
// Conversation list
// ---------------------------------------------------------------------------

fn list() -> impl IntoWidget {
    let rows: Vec<AnyWidget> = store::conversations().iter().map(|c| convo_row(c).into_widget()).collect();

    scaffold(scroll_view(
        column(rows).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min),
    ))
    .top(top_panel("Messages").leading(
        icon_button(lucide::ARROW_LEFT).variant(ButtonVariant::Ghost).on_pressed(store::messages_back),
    ))
}

fn convo_row(convo: &store::Conversation) -> impl IntoWidget {
    let c = theme().colors;
    let other = store::user(convo.user);
    let id = convo.id;
    let last = convo.messages.last();
    let preview = last.map(|m| m.text.clone()).unwrap_or_default();
    let time = last.map(|m| m.time.clone()).unwrap_or_default();
    let unread = convo.unread > 0;

    pressable(
        container().padding(EdgeInsets::symmetric(14.0, 12.0)).child(
            row(children![
                avatar(&other.avatar, 48.0),
                gap_w(12.0),
                Expanded::new(
                    column(children![
                        text(other.name.clone())
                            .size(14.5)
                            .weight(if unread { 700.0 } else { 600.0 })
                            .color(c.foreground),
                        gap_h(3.0),
                        text(preview).size(13.0).max_lines(1).ellipsis().soft_wrap(false).color(if unread {
                            c.foreground
                        } else {
                            c.muted_foreground
                        }),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
                gap_w(8.0),
                column(children![
                    text(time).size(11.5).color(c.muted_foreground),
                    gap_h(6.0),
                    unread_badge(convo.unread),
                ])
                .cross_axis_alignment(CrossAxisAlignment::End)
                .main_axis_size(MainAxisSize::Min),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Center),
        ),
    )
    .on_tap(move || store::open_thread(id))
}

fn unread_badge(n: u32) -> AnyWidget {
    let c = theme().colors;
    if n == 0 {
        return gap_h(0.0).into_widget();
    }
    container()
        .decoration(BoxDecoration::new().color(c.primary).radius(BorderRadius::all(999.0)))
        .padding(EdgeInsets::symmetric(7.0, 2.0))
        .child(text(format!("{n}")).size(11.0).weight(700.0).color(c.primary_foreground))
        .into_widget()
}

// ---------------------------------------------------------------------------
// Thread
// ---------------------------------------------------------------------------

#[allow(clippy::trivially_copy_pass_by_ref)] // `&u64` is required by component_props
fn thread(id: &u64) -> AnyWidget {
    let id = *id;
    let c = theme().colors;
    let draft = create_signal(String::new());

    let convo = store::convo(id);
    let other = store::user(convo.as_ref().map(|c| c.user).unwrap_or(0));
    let msgs = convo.map(|c| c.messages).unwrap_or_default();

    let bubbles: Vec<AnyWidget> = msgs.iter().map(|m| bubble(m).into_widget()).collect();

    let send = move || {
        store::send_message(id, &draft.peek());
        draft.set(String::new());
    };

    let input = container()
        .decoration(BoxDecoration::new().color(c.background).border(Border::new(c.border, 1.0)))
        .padding(EdgeInsets::all(10.0))
        .child(
            row(children![
                Expanded::new(text_field().bind(draft).placeholder("Message…").on_submit(move |_| send())),
                gap_w(8.0),
                icon_button(lucide::SEND).on_pressed(send),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Center),
        );

    scaffold(
        column(children![
            Expanded::new(scroll_view(
                container().padding(EdgeInsets::symmetric(12.0, 10.0)).child(
                    column(bubbles)
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .main_axis_size(MainAxisSize::Min),
                ),
            )),
            input,
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch),
    )
    .top(top_panel(other.name.clone()).leading(
        icon_button(lucide::ARROW_LEFT).variant(ButtonVariant::Ghost).on_pressed(store::messages_back),
    ))
    .into_widget()
}

/// One chat bubble — mine (right, primary) vs theirs (left, secondary).
fn bubble(m: &Message) -> impl IntoWidget {
    let c = theme().colors;
    let mine = m.from == store::ME;
    let (bg, fg) = if mine { (c.primary, c.primary_foreground) } else { (c.secondary, c.foreground) };

    let chip = constrained_box(
        BoxConstraints { min_width: 0.0, max_width: 262.0, min_height: 0.0, max_height: f64::INFINITY },
        container()
            .decoration(BoxDecoration::new().color(bg).radius(BorderRadius::all(16.0)))
            .padding(EdgeInsets::symmetric(13.0, 9.0))
            .child(text(m.text.clone()).size(14.0).line_height(1.35).color(fg)),
    );

    let aligned = if mine { row(children![spacer(), chip]) } else { row(children![chip, spacer()]) };

    container().padding(EdgeInsets::symmetric(0.0, 4.0)).child(aligned)
}
