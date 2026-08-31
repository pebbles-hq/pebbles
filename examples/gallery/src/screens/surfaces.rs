use pebbles::prelude::*;

use crate::ui::{gap_h, screen, section};

pub fn surfaces() -> Element {
    screen("Surfaces")
        .description("Cards, badges, alerts, avatars, separators, skeletons.")
        .body(children![
            section(
                "CARD",
                card().child(
                    column(children![
                        title("Create project"),
                        muted("Deploy your new project in one click."),
                        gap_h(6.0),
                        row(children![
                            button("Deploy"),
                            button("Cancel").variant(ButtonVariant::Ghost)
                        ])
                        .main_axis_size(MainAxisSize::Min)
                        .spacing(10.0),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min)
                    .spacing(8.0)
                ),
            ),
            section(
                "BADGES",
                row(children![
                    badge("Default"),
                    badge("Secondary").variant(BadgeVariant::Secondary),
                    badge("Success").variant(BadgeVariant::Success),
                    badge("Destructive").variant(BadgeVariant::Destructive),
                    badge("Outline").variant(BadgeVariant::Outline),
                ])
                .main_axis_size(MainAxisSize::Min)
                .spacing(8.0),
            ),
            section(
                "ALERTS",
                column(children![
                    alert("Heads up!").description("You can add components using the CLI."),
                    alert("Success")
                        .description("Your changes have been saved.")
                        .variant(AlertVariant::Success),
                    alert("Warning")
                        .description("This action cannot be undone.")
                        .variant(AlertVariant::Warning),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min)
                .spacing(10.0),
            ),
            section(
                "AVATARS",
                row(children![
                    avatar("RS"),
                    avatar("AB").color(palette::BLUE),
                    avatar("JD").color(palette::GREEN),
                    avatar("MK").size(56.0).color(palette::PURPLE),
                ])
                .main_axis_size(MainAxisSize::Min)
                .spacing(12.0),
            ),
            section(
                "SKELETON (shimmer)",
                column(children![
                    skeleton(320.0, 16.0).shimmer(),
                    skeleton(260.0, 16.0).shimmer(),
                    skeleton(190.0, 16.0)
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min)
                .spacing(8.0)
            ),
            section(
                "KBD",
                row(children![kbd("⌘K"), kbd("Ctrl+C"), kbd("⇧⌘P"), kbd("Esc")])
                    .main_axis_size(MainAxisSize::Min)
                    .spacing(8.0)
            ),
            section(
                "EMPTY STATE",
                Container::new().height(220.0).child(
                    empty()
                        .icon(lucide::SEARCH)
                        .title("No results found")
                        .description("Try a different search term.")
                        .action(button("Clear filters").variant(ButtonVariant::Outline)),
                ),
            ),
        ])
}
