use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn skeletons() -> Element {
    screen("Skeleton").body(children![
        doc("Lines").body(
            column(children![skeleton(320.0, 16.0), skeleton(280.0, 16.0), skeleton(200.0, 16.0),])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min)
                .spacing(10.0),
        ),
        doc("Shimmer vs static").body(
            column(children![
                row(children![
                    text("shimmer").size(12.0).color(theme().colors.muted_foreground),
                    gap_w(12.0),
                    skeleton(160.0, 16.0).shimmer(),
                ])
                .main_axis_size(MainAxisSize::Min),
                gap_h(10.0),
                row(children![
                    text("static ").size(12.0).color(theme().colors.muted_foreground),
                    gap_w(12.0),
                    skeleton(160.0, 16.0),
                ])
                .main_axis_size(MainAxisSize::Min),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        ),
        doc("Card skeleton").body(
            card().child(
                column(children![
                    row(children![
                        skeleton(44.0, 44.0).shimmer(),
                        gap_w(12.0),
                        column(
                            children![skeleton(180.0, 14.0).shimmer(), gap_h(8.0), skeleton(120.0, 14.0),]
                        )
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .main_axis_size(MainAxisSize::Min),
                    ])
                    .main_axis_size(MainAxisSize::Min),
                    gap_h(16.0),
                    skeleton(300.0, 12.0),
                    gap_h(6.0),
                    skeleton(260.0, 12.0),
                    gap_h(16.0),
                    row(children![skeleton(90.0, 28.0).shimmer(), gap_w(8.0), skeleton(90.0, 28.0),])
                        .main_axis_size(MainAxisSize::Min),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min),
            ),
        ),
        doc("Profile row").body(
            column(children![
                profile_row(true),
                separator(),
                profile_row(false),
                separator(),
                profile_row(true),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        ),
    ])
}

fn profile_row(shimmer: bool) -> impl IntoWidget {
    let avatar_block = if shimmer { skeleton(36.0, 36.0).shimmer() } else { skeleton(36.0, 36.0) };
    row(children![
        avatar_block,
        gap_w(10.0),
        column(children![skeleton(140.0, 13.0).shimmer(), gap_h(6.0), skeleton(90.0, 11.0),])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
    ])
    .main_axis_size(MainAxisSize::Min)
}
