use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn badges() -> Element {
    screen("Badge").body(children![
        doc("Variants").body(
            wrap(children![
                badge("Default"),
                badge("Secondary").variant(BadgeVariant::Secondary),
                badge("Success").variant(BadgeVariant::Success),
                badge("Destructive").variant(BadgeVariant::Destructive),
                badge("Outline").variant(BadgeVariant::Outline),
            ])
            .spacing(8.0),
        ),
        doc("Styled").body(
            wrap(children![
                badge("Pebbles").style(style().background(palette::violet::S600).radius_all(999.0)),
                badge("Beta").style(style().background(palette::amber::S500).radius_all(4.0)),
                badge("Cloud").style(style().background(palette::sky::S500).radius_all(999.0)),
                badge("Outline tint")
                    .variant(BadgeVariant::Outline)
                    .style(style().background(theme().colors.card)),
            ])
            .spacing(8.0),
        ),
        doc("In context").body(
            column(children![
                status_row("Build #42", badge("Passing").variant(BadgeVariant::Success)),
                status_row("Build #43", badge("Failing").variant(BadgeVariant::Destructive)),
                status_row("Build #44", badge("Pending").variant(BadgeVariant::Secondary)),
                status_row("Build #45", badge("Skipped").variant(BadgeVariant::Outline)),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(10.0),
        ),
        doc("Counts & icon chips").body(
            wrap(children![
                badge("12"),
                badge("3").variant(BadgeVariant::Outline),
                badge("99+").variant(BadgeVariant::Destructive),
                container()
                    .decoration(
                        BoxDecoration::new().color(theme().colors.secondary).radius(BorderRadius::all(999.0))
                    )
                    .padding(EdgeInsets::symmetric(8.0, 2.0))
                    .child(
                        row(children![
                            icon(tabler::SPARKLES).size(12.0).color(theme().colors.muted_foreground),
                            gap_w(4.0),
                            text("New").size(11.0).weight(500.0),
                        ])
                        .main_axis_size(MainAxisSize::Min),
                    ),
            ])
            .spacing(8.0),
        ),
    ])
}

fn status_row(build: &str, pill: impl IntoWidget) -> impl IntoWidget {
    row(children![text(build.to_string()).size(14.0).weight(500.0), gap_w(8.0), pill,])
        .main_axis_size(MainAxisSize::Min)
}
