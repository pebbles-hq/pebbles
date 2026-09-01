//! Sheet & Drawer: edge-anchored modal panels.

use pebbles::prelude::*;

use crate::ui::{doc, gap_w, screen};

pub fn sheets() -> Element {
    screen("Sheet & Drawer")
        .description(
            "Edge-anchored modal panels over a dimmed scrim: Left/Right for side sheets, Top/Bottom for drawers. Escape or an outside click dismisses. Sizes are the extent along their edge.",
        )
        .body(children![right(), bottom(), left(), top()])
}

fn right() -> impl IntoWidget {
    doc("Right sheet")
        .description("The classic filter/settings panel: full height, slides from the right edge, a title header and a scrollable body.")
        .body(
            row(children![
                button("Open right sheet").variant(ButtonVariant::Outline).on_pressed(|| {
                    sheet(
                        column(children![
                            muted("Filter the results by the fields below.").size(12.5),
                            gap_w(14.0),
                            text_field().placeholder("Search").width(280.0),
                            gap_w(10.0),
                            text_field().placeholder("Owner").width(280.0),
                            gap_w(10.0),
                            text_field().placeholder("Date range").width(280.0),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .main_axis_size(MainAxisSize::Min),
                    )
                    .side(Side::Right)
                    .size(340.0)
                    .title("Filters")
                    .open();
                }),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn bottom() -> impl IntoWidget {
    doc("Bottom drawer")
        .description("Slides up from the bottom edge — the mobile-style drawer for details, actions and quick inputs.")
        .body(
            row(children![
                button("Open bottom drawer").variant(ButtonVariant::Outline).on_pressed(|| {
                    sheet(
                        column(children![
                            muted("A drawer slides up from the bottom edge.").size(12.5),
                            gap_w(12.0),
                            text_field().placeholder("Quick note…").width(320.0),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .main_axis_size(MainAxisSize::Min),
                    )
                    .side(Side::Bottom)
                    .size(200.0)
                    .title("Details")
                    .open();
                }),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn left() -> impl IntoWidget {
    doc("Left sheet")
        .description("The navigation drawer side — same panel contract, mirrored edge.")
        .body(
            row(children![
                button("Open left sheet").variant(ButtonVariant::Outline).on_pressed(|| {
                    sheet(
                        column(children![
                            muted("Navigation lives here in many apps.").size(12.5),
                            gap_w(10.0),
                            row(children![
                                icon(IconKind::User).size(16.0),
                                gap_w(10.0),
                                text("Profile").size(13.5),
                            ])
                            .main_axis_size(MainAxisSize::Min),
                            row(children![
                                icon(IconKind::Star).size(16.0),
                                gap_w(10.0),
                                text("Starred").size(13.5),
                            ])
                            .main_axis_size(MainAxisSize::Min),
                            row(children![
                                icon(IconKind::Folder).size(16.0),
                                gap_w(10.0),
                                text("Projects").size(13.5),
                            ])
                            .main_axis_size(MainAxisSize::Min),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .main_axis_size(MainAxisSize::Min)
                        .spacing(8.0),
                    )
                    .side(Side::Left)
                    .size(260.0)
                    .title("Menu")
                    .open();
                }),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn top() -> impl IntoWidget {
    doc("Top sheet")
        .description("Drops from the top edge — a compact surface for a toolbar, a confirmation or a quick action strip.")
        .body(
            row(children![
                button("Open top sheet").variant(ButtonVariant::Outline).on_pressed(|| {
                    sheet(muted("A top sheet drops from the top edge.").size(12.5))
                        .side(Side::Top)
                        .size(120.0)
                        .title("Command strip")
                        .open();
                }),
                gap_w(10.0),
                muted("sides: Right · Bottom · Left · Top").size(12.0),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}
