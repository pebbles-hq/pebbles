use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn lists() -> Element {
    let picked = create_signal(String::from("—"));
    let sel = create_signal(1usize);

    screen("List")
        .description("List rows — shadcn's Item: a leading widget, a title + optional subtitle, and a trailing widget. Every slot takes a widget, rows are clickable and selectable, and a universal Style covers the surface + title text.")
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
            doc("Interactive")
                .description(".on_tap(..) makes the row clickable — hover feedback, pointer cursor, and the tap reports out (semantics announce it as a button).")
                .body(
                    column(children![
                        card().child(
                            column(children![
                                list_tile("Open documentation").leading(icon(IconKind::Star).size(18.0)).subtitle("docs.pebbles.dev").on_tap(move || picked.set("documentation".into())),
                                separator(),
                                list_tile("Report an issue").leading(icon(IconKind::Warning).size(18.0)).subtitle("GitHub issues").on_tap(move || picked.set("issues".into())),
                                separator(),
                                list_tile("Unavailable").subtitle("disabled rows never fire").on_tap(move || picked.set("never".into())).disabled(true),
                            ])
                            .cross_axis_alignment(CrossAxisAlignment::Stretch)
                            .main_axis_size(MainAxisSize::Min),
                        )
                        .padding(EdgeInsets::all(4.0)),
                        gap_h(8.0),
                        muted(format!("last tap: {}", picked.get())),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("Selected state")
                .description(".selected(true) + .selected_color(..) tint the row — pair with .on_tap for a controlled choice list.")
                .body(
                    column(children![
                        card().child(
                            column(children![
                                choice_row(0, "Light", sel.get(), move || sel.set(0)),
                                separator(),
                                choice_row(1, "Dark", sel.get(), move || sel.set(1)),
                                separator(),
                                choice_row(2, "System", sel.get(), move || sel.set(2)),
                            ])
                            .cross_axis_alignment(CrossAxisAlignment::Stretch)
                            .main_axis_size(MainAxisSize::Min),
                        )
                        .padding(EdgeInsets::all(4.0)),
                        gap_h(8.0),
                        muted(format!("selected: {}", ["Light", "Dark", "System"][sel.get()])),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("Styled surface")
                .description("A Style covers the surface — background, border, radius, shadow, min-height — merged over the tile's base (user wins).")
                .body(
                    column(children![
                        list_tile("Card background").subtitle("bg + radius from a Style")
                            .leading(avatar("RS"))
                            .style(style().background(theme().colors.card).radius_all(theme().radius).min_height(56.0)),
                        gap_h(8.0),
                        list_tile("Bordered").subtitle("border + custom padding")
                            .style(style().background(palette::sky::S600).radius_all(6.0)),
                        gap_h(8.0),
                        list_tile("Accent tint").subtitle("any color from the palette")
                            .style(style().background(palette::emerald::S600).radius_all(6.0)),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("Styled text")
                .description("The Style's text props (color, size, weight) drive the title — typography is one Style away.")
                .body(
                    column(children![
                        list_tile("Big & bold").subtitle("size 20 / weight 700")
                            .style(style().font_size(20.0).font_weight(700.0).color(palette::violet::S600)),
                        gap_h(8.0),
                        list_tile("Accent title").subtitle("the subtitle stays muted")
                            .style(style().color(palette::blue::S600)),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("Density & gaps")
                .description(".dense(true) compacts the row; .content_padding(..) and .leading_gap(..) fine-tune the layout.")
                .body(
                    card().child(
                        column(children![
                            list_tile("Default density").subtitle("padding (12, 10)"),
                            separator(),
                            list_tile("Dense").subtitle("padding (12, 6)").dense(true),
                            separator(),
                            list_tile("Custom padding").subtitle("content_padding(EdgeInsets::symmetric(20, 14))").content_padding(EdgeInsets::symmetric(20.0, 14.0)),
                            separator(),
                            list_tile("Wide leading gap").subtitle("leading_gap(28)").leading(icon(IconKind::Dot).size(18.0)).leading_gap(28.0),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .main_axis_size(MainAxisSize::Min),
                    )
                    .padding(EdgeInsets::all(4.0)),
                ),
        ])
}

fn choice_row(index: usize, label: &str, current: usize, pick: impl Fn() + 'static) -> impl IntoWidget {
    list_tile(label)
        .subtitle("a controlled choice")
        .trailing(if current == index {
            icon(IconKind::Check).size(16.0).color(theme().colors.primary).into_widget()
        } else {
            gap_w(0.0).into_widget()
        })
        .selected(current == index)
        .on_tap(pick)
}

fn member_row(initials: &str, name: &str, role: &str, status: BadgeVariant) -> impl IntoWidget {
    list_tile(name)
        .leading(avatar(initials.to_string()).color(palette::BLUE))
        .subtitle(role)
        .trailing(badge(if matches!(status, BadgeVariant::Destructive) { "Away" } else { "Active" }).variant(status))
}
