use pebbles::prelude::*;

use crate::ui::{gap_h, hstack, screen, section, vstack};

pub fn surfaces() -> impl IntoWidget {
    screen(
        "Surfaces",
        "Cards, badges, alerts, avatars, separators, skeletons.",
        children![
            section(
                "CARD",
                Card::new(vstack(
                    children![
                        title("Create project"),
                        muted("Deploy your new project in one click."),
                        gap_h(6.0),
                        hstack(children![button("Deploy"), button("Cancel").variant(ButtonVariant::Ghost)], 10.0),
                    ],
                    8.0,
                )),
            ),
            section(
                "BADGES",
                hstack(
                    children![
                        badge("Default"),
                        badge("Secondary").variant(BadgeVariant::Secondary),
                        badge("Success").variant(BadgeVariant::Success),
                        badge("Destructive").variant(BadgeVariant::Destructive),
                        badge("Outline").variant(BadgeVariant::Outline),
                    ],
                    8.0,
                ),
            ),
            section(
                "ALERTS",
                vstack(
                    children![
                        alert("Heads up!", "You can add components using the CLI."),
                        alert("Success", "Your changes have been saved.").variant(AlertVariant::Success),
                        alert("Warning", "This action cannot be undone.").variant(AlertVariant::Warning),
                    ],
                    10.0,
                ),
            ),
            section(
                "AVATARS",
                hstack(
                    children![
                        avatar("RS"),
                        avatar("AB").color(palette::BLUE),
                        avatar("JD").color(palette::GREEN),
                        avatar("MK").size(56.0).color(palette::PURPLE),
                    ],
                    12.0,
                ),
            ),
            section("SKELETON", vstack(children![skeleton(320.0, 16.0), skeleton(260.0, 16.0), skeleton(190.0, 16.0)], 8.0)),
        ],
    )
}
