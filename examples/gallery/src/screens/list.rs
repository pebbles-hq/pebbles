use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn lists() -> Element {
    screen("List")
        .description("List rows — shadcn's Item: a leading widget, a title + optional subtitle, and a trailing widget. Compose anything into the slots.")
        .body(children![
            doc("Basic rows")
                .description("Title only, or with a subtitle — the plain text row.")
                .body(
                    card().child(
                        column(children![
                            list_tile("Inbox"),
                            separator(),
                            list_tile("Starred"),
                            separator(),
                            list_tile("Drafts"),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .main_axis_size(MainAxisSize::Min),
                    )
                    .padding(EdgeInsets::all(4.0)),
                ),
            doc("Leading + trailing")
                .description("Icons lead, badges or buttons trail — the full row.")
                .body(
                    card().child(
                        column(children![
                            list_tile("Inbox").leading(icon(IconKind::Mail).size(18.0)).subtitle("12 new").trailing(badge("12")),
                            separator(),
                            list_tile("Downloads").leading(icon(IconKind::Search).size(18.0)).subtitle("3 in progress").trailing(badge("3").variant(BadgeVariant::Secondary)),
                            separator(),
                            list_tile("Trash").leading(icon(IconKind::Close).size(18.0)).trailing(button("Empty").variant(ButtonVariant::Ghost).size(ButtonSize::Sm)),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .main_axis_size(MainAxisSize::Min),
                    )
                    .padding(EdgeInsets::all(4.0)),
                ),
            doc("Controls in rows")
                .description("Toggle controls read perfectly as trailing widgets.")
                .body(
                    card().child(
                        column(children![
                            list_tile("Notifications").subtitle("Email, mentions, replies").trailing(switch(true).on_changed(|| {})),
                            separator(),
                            list_tile("Sound effects").subtitle("Play a sound on events").trailing(switch(false).on_changed(|| {})),
                            separator(),
                            list_tile("Keep me signed in").trailing(checkbox(true).on_changed(|| {})),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .main_axis_size(MainAxisSize::Min),
                    )
                    .padding(EdgeInsets::all(4.0)),
                ),
            doc("Status rows")
                .description("Avatars + status pills — the team-member list.")
                .body(
                    card().child(
                        column(children![
                            member_row("RS", "Reyco Seguma", "Lead", BadgeVariant::Default),
                            separator(),
                            member_row("AK", "Andres King", "Engineer", BadgeVariant::Success),
                            separator(),
                            member_row("JB", "Joseph Bello", "Engineer", BadgeVariant::Destructive),
                            separator(),
                            member_row("MK", "Marvin Kato", "Design", BadgeVariant::Secondary),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .main_axis_size(MainAxisSize::Min),
                    )
                    .padding(EdgeInsets::all(4.0)),
                ),
        ])
}

fn member_row(initials: &str, name: &str, role: &str, status: BadgeVariant) -> impl IntoWidget {
    list_tile(name)
        .leading(avatar(initials.to_string()).color(palette::BLUE))
        .subtitle(role)
        .trailing(badge(if matches!(status, BadgeVariant::Destructive) { "Away" } else { "Active" }).variant(status))
}
