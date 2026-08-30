use pebbles::prelude::*;

use crate::ui::{gap_h, screen, section};

pub fn surfaces() -> Element {
    screen(
        "Surfaces",
        "Cards, badges, alerts, avatars, separators, skeletons.",
        children![
            section(
                "CARD",
                Card::new(column(
                    children![
                        title("Create project"),
                        muted("Deploy your new project in one click."),
                        gap_h(6.0),
                        row(children![button("Deploy"), button("Cancel").variant(ButtonVariant::Ghost)]).min().spacing(10.0),
                    ]).start().min().spacing(8.0)),
            ),
            section(
                "BADGES",
                row(
                    children![
                        badge("Default"),
                        badge("Secondary").variant(BadgeVariant::Secondary),
                        badge("Success").variant(BadgeVariant::Success),
                        badge("Destructive").variant(BadgeVariant::Destructive),
                        badge("Outline").variant(BadgeVariant::Outline),
                    ]).min().spacing(8.0),
            ),
            section(
                "ALERTS",
                column(
                    children![
                        alert("Heads up!", "You can add components using the CLI."),
                        alert("Success", "Your changes have been saved.").variant(AlertVariant::Success),
                        alert("Warning", "This action cannot be undone.").variant(AlertVariant::Warning),
                    ]).start().min().spacing(10.0),
            ),
            section(
                "AVATARS",
                row(
                    children![
                        avatar("RS"),
                        avatar("AB").color(palette::BLUE),
                        avatar("JD").color(palette::GREEN),
                        avatar("MK").size(56.0).color(palette::PURPLE),
                    ]).min().spacing(12.0),
            ),
            section("SKELETON (shimmer)", column(children![skeleton(320.0, 16.0).shimmer(), skeleton(260.0, 16.0).shimmer(), skeleton(190.0, 16.0)]).start().min().spacing(8.0)),
            section("KBD", row((kbd("⌘K"), kbd("Ctrl+C"), kbd("⇧⌘P"), kbd("Esc"))).min().spacing(8.0)),
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
        ],
    )
}
