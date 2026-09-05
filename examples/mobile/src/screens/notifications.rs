//! The notifications tab — a list of alerts with unread highlighting and a
//! "Mark all read" action (which clears the tab badge).

use pebbles::prelude::*;

use crate::components::bits::avatar;
use crate::store::{self, Notif, NotifKind};

pub fn notifications() -> impl IntoWidget {
    let c = theme().colors;
    let list = store::notifs().get();

    let header = row(children![
        text("Notifications").size(18.0).semibold().color(c.foreground),
        spacer(),
        button("Mark all read").variant(ButtonVariant::Ghost).on_pressed(store::mark_all_read),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center);

    let mut kids: Vec<AnyWidget> =
        vec![container().padding(EdgeInsets::symmetric(14.0, 12.0)).child(header).into_widget()];
    kids.extend(list.iter().map(|n| row_of(n).into_widget()));

    scroll_view(
        column(kids).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min),
    )
}

fn row_of(n: &Notif) -> impl IntoWidget {
    let c = theme().colors;
    let actor = store::user(n.actor);
    let verb = match n.kind {
        NotifKind::Like => "liked your post",
        NotifKind::Comment => "commented on your post",
        NotifKind::Follow => "started following you",
    };
    // Unread rows get a faint accent wash + a dot.
    let bg = if n.read { c.background } else { mix_accent(c.background, c.accent) };

    container().color(bg).padding(EdgeInsets::symmetric(14.0, 12.0)).child(
        row(children![
            avatar(&actor.avatar, 40.0),
            gap_w(12.0),
            Expanded::new(
                text_rich(vec![
                    span(actor.name.clone()).weight(600.0).color(c.foreground),
                    span(format!(" {verb}")).color(c.foreground),
                ])
                .size(14.0),
            ),
            gap_w(8.0),
            text(n.time.clone()).size(12.0).color(c.muted_foreground),
            gap_w(8.0),
            unread_dot(!n.read),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center),
    )
}

fn unread_dot(unread: bool) -> impl IntoWidget {
    let c = theme().colors;
    let color = if unread { c.primary } else { palette::TRANSPARENT };
    container().decoration(BoxDecoration::new().color(color).shape(BoxShape::Circle)).width(8.0).height(8.0)
}

/// A subtle accent wash for unread rows.
fn mix_accent(bg: Color, accent: Color) -> Color {
    let [ar, ag, ab, _] = accent.components;
    let [br, bg_, bb, _] = bg.components;
    let t = 0.08;
    Color::new([br + (ar - br) * t, bg_ + (ag - bg_) * t, bb + (ab - bb) * t, 1.0])
}
