//! The notifications bell + its dropdown popover.

use pebbles::prelude::*;

use super::mix;
use crate::store::{self, NotifKind};

/// The top-bar bell (with an unread dot) that opens the notifications popover.
pub fn notifications_button() -> AnyWidget {
    let c = theme().colors;
    let unread = store::unread_notifs();
    let glyph: AnyWidget = if unread > 0 {
        stack(children![
            icon(lucide::BELL).size(20.0).color(c.foreground),
            positioned(
                container()
                    .decoration(BoxDecoration::new().color(palette::rose::S500).shape(BoxShape::Circle))
                    .width(8.0)
                    .height(8.0),
            )
            .right(0.0)
            .top(0.0),
        ])
        .into_widget()
    } else {
        icon(lucide::BELL).size(20.0).color(c.foreground).into_widget()
    };
    let trigger = container().padding(EdgeInsets::all(8.0)).child(glyph);
    // pad(0) so the header divider and row separators run edge to edge.
    popover(panel(), trigger).width(360.0).height(420.0).trigger_height(38.0).pad(0.0).into_widget()
}

fn panel() -> AnyWidget {
    let c = theme().colors;
    let notifs = store::notifications();
    let unread = store::unread_notifs();

    // Header: title (+ unread count) and "Mark all read" when there's something unread.
    let title = if unread > 0 { format!("Notifications · {unread}") } else { "Notifications".to_string() };
    let mut head: Vec<AnyWidget> =
        vec![text(title).size(14.0).weight(700.0).color(c.foreground).into_widget(), spacer().into_widget()];
    if unread > 0 {
        head.push(
            pressable(text("Mark all read").size(12.5).weight(500.0).color(c.primary))
                .radius(6.0)
                .on_tap(store::mark_notifs_read)
                .into_widget(),
        );
    }
    let header = container()
        .padding(EdgeInsets::symmetric(14.0, 12.0))
        .child(row(head).cross_axis_alignment(CrossAxisAlignment::Center));

    // Body: rows separated by hairlines, or an empty state. Sizes to content for a few
    // notifications; caps to a scrollable height when there are many.
    let body: AnyWidget = if notifs.is_empty() {
        container()
            .padding(EdgeInsets::symmetric(14.0, 36.0))
            .alignment(Alignment::CENTER)
            .child(text("You're all caught up ✦").size(13.0).color(c.muted_foreground))
            .into_widget()
    } else {
        let mut kids: Vec<AnyWidget> = Vec::new();
        for (i, n) in notifs.iter().enumerate() {
            if i > 0 {
                kids.push(container().color(c.border).height(1.0).into_widget());
            }
            kids.push(row_of(n));
        }
        let list =
            column(kids).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min);
        if notifs.len() > 6 {
            container().height(360.0).child(scroll_view(list).drag_scroll(true)).into_widget()
        } else {
            list.into_widget()
        }
    };

    column(children![header, container().color(c.border).height(1.0), body])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
}

fn row_of(n: &store::Notif) -> AnyWidget {
    let c = theme().colors;
    let (ic, color) = match n.kind {
        NotifKind::LowStock => (lucide::PACKAGE, palette::amber::S500),
        NotifKind::Order => (lucide::SHOPPING_CART, palette::sky::S500),
        NotifKind::Sync => (lucide::CLOUD_DOWNLOAD, palette::emerald::S500),
        NotifKind::Info => (lucide::BELL, palette::violet::S500),
    };
    let unread = !n.read;
    // Unread rows get a faint accent wash across the full width.
    let bg = if unread { mix(c.card, c.accent, 0.5) } else { palette::TRANSPARENT };
    // Trailing dot marks unread rows (kept as a gap when read, to hold alignment).
    let dot: AnyWidget = if unread {
        container()
            .decoration(BoxDecoration::new().color(c.primary).shape(BoxShape::Circle))
            .width(7.0)
            .height(7.0)
            .into_widget()
    } else {
        gap_h(7.0).into_widget()
    };

    container()
        .color(bg)
        .padding(EdgeInsets::symmetric(14.0, 11.0))
        .child(
            row(children![
                container()
                    .decoration(
                        BoxDecoration::new().color(mix(c.card, color, 0.16)).radius(BorderRadius::all(9.0)),
                    )
                    .padding(EdgeInsets::all(8.0))
                    .child(icon(ic).size(15.0).color(color)),
                gap_w(11.0),
                Expanded::new(
                    column(children![
                        text(n.title.clone()).size(13.0).weight(600.0).color(c.foreground),
                        gap_h(2.0),
                        text(n.body.clone()).size(12.0).line_height(1.4).color(c.muted_foreground),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
                gap_w(10.0),
                column(children![text(n.time.clone()).size(11.0).color(c.muted_foreground), gap_h(6.0), dot])
                    .cross_axis_alignment(CrossAxisAlignment::End)
                    .main_axis_size(MainAxisSize::Min),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start),
        )
        .into_widget()
}
