//! Popover: click-triggered floating panels in the overlay layer.

use pebbles::prelude::*;

use crate::ui::{doc, gap_w, screen};

pub fn popovers() -> Element {
    screen("Popover")
        .description(
            "Click a trigger to float arbitrary content in the overlay layer: real inputs, layout, anything. Popovers flip near screen edges, follow page scroll, and dismiss on outside click or Escape.",
        )
        .body(children![form(), sized(), styled(), wide()])
}

fn form() -> impl IntoWidget {
    doc("Form popover")
        .description(
            "The canonical settings flyout — a titled panel with live inputs, anchored to its trigger.",
        )
        .body(
            row(children![
                popover(
                    column(children![
                        text("Dimensions").size(14.0).semibold(),
                        gap_w(0.0),
                        muted("Set the width and height of the panel.").size(12.0),
                        gap_w(8.0),
                        text_field().placeholder("Width").width(200.0),
                        gap_w(6.0),
                        text_field().placeholder("Height").width(200.0),
                        gap_w(10.0),
                        row(children![
                            button("Apply").size(ButtonSize::Sm).on_click(|| {}),
                            button("Cancel")
                                .size(ButtonSize::Sm)
                                .variant(ButtonVariant::Ghost)
                                .on_click(|| {}),
                        ])
                        .main_axis_size(MainAxisSize::Min)
                        .spacing(6.0),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                    button("Open popover").variant(ButtonVariant::Outline),
                )
                .width(232.0)
                .height(210.0)
                .trigger_height(38.0),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn sized() -> impl IntoWidget {
    doc("Sizing")
        .description(".width(..) / .height(..) set the panel; .trigger_height(..) the trigger. The panel wraps your content — size it to the content you host.")
        .body(
            row(children![
                popover(
                    column(children![
                        text("Compact").semibold().size(13.0),
                        muted("A small panel for a small choice.").size(11.5),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                    button("Compact").variant(ButtonVariant::Outline),
                )
                .width(180.0)
                .height(90.0),
                gap_w(12.0),
                popover(
                    column(children![
                        text("Roomy").semibold().size(13.0),
                        muted("Panels grow as wide as their content needs.").size(11.5),
                        gap_w(6.0),
                        row(children![
                            toggle(false, text("Drafts").size(13.0)),
                            toggle(true, text("Published").size(13.0)),
                        ])
                        .main_axis_size(MainAxisSize::Min)
                        .spacing(10.0),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                    button("Roomy").variant(ButtonVariant::Outline),
                )
                .width(260.0)
                .height(160.0),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn styled() -> impl IntoWidget {
    doc("Styled")
        .description(".style(..) restyles the panel chrome — background, border, radius — independently of the content.")
        .body(
            row(children![
                popover(
                    column(children![
                        text("Themed panel").semibold().size(13.0),
                        muted("Restyle without touching the behavior.").size(11.5),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                    button("Styled").variant(ButtonVariant::Outline),
                )
                .width(220.0)
                .height(110.0)
                .style(
                    style()
                        .background(theme().colors.card)
                        .border(Border::new(theme().colors.primary, 1.5))
                        .radius_all(12.0)
                        .shadow(BoxShadow::new(Color::from_rgba8(0, 0, 0, 40), Offset::new(0.0, 4.0), 12.0, 0.0)),
                ),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn wide() -> impl IntoWidget {
    doc("Layout inside")
        .description("Panels host full layouts — here a two-column field grid, like a date-range picker or a table-filter flyout.")
        .body(
            row(children![
                popover(
                    column(children![
                        text("Filters").semibold().size(14.0),
                        gap_w(8.0),
                        row(children![
                            column(children![
                                muted("From").size(11.0),
                                text_field().placeholder("2024-01-01").width(130.0),
                            ])
                            .cross_axis_alignment(CrossAxisAlignment::Start)
                            .main_axis_size(MainAxisSize::Min),
                            gap_w(10.0),
                            column(children![
                                muted("To").size(11.0),
                                text_field().placeholder("2024-12-31").width(130.0),
                            ])
                            .cross_axis_alignment(CrossAxisAlignment::Start)
                            .main_axis_size(MainAxisSize::Min),
                        ])
                        .main_axis_size(MainAxisSize::Min),
                        gap_w(8.0),
                        select(["All statuses", "Draft", "Published", "Archived"])
                            .width(260.0),
                        gap_w(8.0),
                        button("Apply filters").on_click(|| {}),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                    button("Filter table").variant(ButtonVariant::Outline),
                )
                .width(320.0)
                .height(230.0),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}
