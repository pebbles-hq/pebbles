use pebbles::prelude::*;

use crate::ui::{doc, gap_w, screen};

pub fn separators() -> Element {
    screen("Separator").body(children![horizontal(), vertical_sep(), thickness(), colors(), labeled()])
}

fn horizontal() -> impl IntoWidget {
    doc("Horizontal").body(
        container().width(360.0).child(
            column(children![
                text("Pebbles").size(15.0).semibold(),
                muted("A Rust GUI framework."),
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
    doc("Vertical").body(
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
    doc("Thickness").body(
        container().width(360.0).child(
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
    doc("Color").body(
        container().width(360.0).child(
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
    doc("With a label").body(
        container().width(360.0).child(
            row(children![
                expanded(separator()),
                gap_w(12.0),
                muted("OR"),
                gap_w(12.0),
                expanded(separator()),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Center),
        ),
    )
}
