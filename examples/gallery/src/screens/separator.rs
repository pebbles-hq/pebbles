use pebbles::prelude::*;

use crate::ui::{doc, gap_w, screen};

pub fn separators() -> Element {
    screen(
        "Separator",
        "A hairline divider (shadcn's Separator) — horizontal or vertical, with adjustable thickness and color.",
        children![horizontal(), vertical_sep(), thickness(), colors(), labeled()],
    )
}

fn horizontal() -> impl IntoWidget {
    doc(
        "Horizontal",
        "separator() fills the available width; a horizontal rule between stacked content.",
        Container::new().width(360.0).child(
            column(children![
                text("Pebbles").size(15.0).semibold(),
                muted("A Flutter-style Rust GUI framework."),
                SizedBox::spacer(0.0, 12.0),
                separator(),
                SizedBox::spacer(0.0, 12.0),
                muted("Everything below the line."),
            ])
            .start()
            .min(),
        ),
    )
}

fn vertical_sep() -> impl IntoWidget {
    doc(
        "Vertical",
        "Separator::vertical() divides items in a row — give it a length or place it in a bounded row.",
        row(children![
            text("Docs").size(14.0),
            Separator::vertical().length(16.0),
            text("API").size(14.0),
            Separator::vertical().length(16.0),
            text("Source").size(14.0),
        ])
        .min()
        .spacing(14.0),
    )
}

fn thickness() -> impl IntoWidget {
    doc(
        "Thickness",
        "Thicken the rule with .thickness().",
        Container::new().width(360.0).child(
            column(children![
                separator().thickness(1.0),
                SizedBox::spacer(0.0, 16.0),
                separator().thickness(2.0),
                SizedBox::spacer(0.0, 16.0),
                separator().thickness(4.0),
            ])
            .stretch()
            .min(),
        ),
    )
}

fn colors() -> impl IntoWidget {
    doc(
        "Color",
        "Recolor with .color() — defaults to the theme border.",
        Container::new().width(360.0).child(
            column(children![
                separator().thickness(2.0).color(palette::emerald::S500),
                SizedBox::spacer(0.0, 16.0),
                separator().thickness(2.0).color(palette::blue::S500),
                SizedBox::spacer(0.0, 16.0),
                separator().thickness(2.0).color(palette::rose::S500),
            ])
            .stretch()
            .min(),
        ),
    )
}

fn labeled() -> impl IntoWidget {
    doc(
        "With a label",
        "Compose a labeled divider — a separator on each side of centered text.",
        Container::new().width(360.0).child(
            row(children![
                Expanded::new(separator()),
                gap_w(12.0),
                muted("OR"),
                gap_w(12.0),
                Expanded::new(separator()),
            ])
            .center_cross(),
        ),
    )
}
