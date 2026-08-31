use pebbles::prelude::*;

use crate::ui::{doc, gap_w, screen};

pub fn separators() -> Element {
    screen("Separator")
        .description("A hairline divider (shadcn's Separator) — horizontal or vertical, with adjustable thickness and color.")
        .body(
        children![horizontal(), vertical_sep(), thickness(), colors(), labeled()],
    )
}

fn horizontal() -> impl IntoWidget {
    doc("Horizontal")
        .description(
            "separator() fills the available width; a horizontal rule between stacked content.",
        )
        .body(
            Container::new().width(360.0).child(
                column(children![
                    text("Pebbles").size(15.0).semibold(),
                    muted("A Flutter-style Rust GUI framework."),
                    gap_h(12.0),
                    separator(),
                    gap_h(12.0),
                    muted("Everything below the line."),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min),
            ),
        )
}

fn vertical_sep() -> impl IntoWidget {
    doc("Vertical")
        .description("Separator::vertical() divides items in a row — give it a length or place it in a bounded row.")
        .body(
        row(children![
            text("Docs").size(14.0),
            Separator::vertical().length(16.0),
            text("API").size(14.0),
            Separator::vertical().length(16.0),
            text("Source").size(14.0),
        ])
        .main_axis_size(MainAxisSize::Min)
        .spacing(14.0),
    )
}

fn thickness() -> impl IntoWidget {
    doc("Thickness")
        .description("Thicken the rule with .thickness().")
        .body(
            Container::new().width(360.0).child(
                column(children![
                    separator().thickness(1.0),
                    gap_h(16.0),
                    separator().thickness(2.0),
                    gap_h(16.0),
                    separator().thickness(4.0),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_size(MainAxisSize::Min),
            ),
        )
}

fn colors() -> impl IntoWidget {
    doc("Color")
        .description("Recolor with .color() — defaults to the theme border.")
        .body(
            Container::new().width(360.0).child(
                column(children![
                    separator().thickness(2.0).color(palette::emerald::S500),
                    gap_h(16.0),
                    separator().thickness(2.0).color(palette::blue::S500),
                    gap_h(16.0),
                    separator().thickness(2.0).color(palette::rose::S500),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_size(MainAxisSize::Min),
            ),
        )
}

fn labeled() -> impl IntoWidget {
    doc("With a label")
        .description("Compose a labeled divider — a separator on each side of centered text.")
        .body(
            Container::new().width(360.0).child(
                row(children![
                    Expanded::new(separator()),
                    gap_w(12.0),
                    muted("OR"),
                    gap_w(12.0),
                    Expanded::new(separator()),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Center),
            ),
        )
}
