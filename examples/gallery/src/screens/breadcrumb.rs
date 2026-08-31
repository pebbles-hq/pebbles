use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn breadcrumbs() -> Element {
    screen("Breadcrumb")
        .description("A trail of path segments — a plain row with a glyph between segments. Long trails collapse into a '…' dropdown; the separator and the segment text are customizable.")
        .body(children![
            doc("Basic")
                .description("breadcrumb(vec![..]) — the plain trail.")
                .body(
                    breadcrumb(vec!["Home".into(), "Projects".into(), "Pebbles".into()]),
                ),
            doc("Ellipsis — max_visible")
                .description(".max_visible(n) collapses the middle segments into a '…' dropdown that lists them.")
                .body(
                    column(children![
                        breadcrumb(
                            ["Home", "Workspace", "Design", "Components", "Input", "Select", "pebbles.rs"]
                                .into_iter()
                                .map(String::from)
                                .collect(),
                        )
                        .max_visible(4),
                        gap_h(16.0),
                        breadcrumb(
                            ["Home", "Settings", "Billing", "Invoices", "2026", "August"]
                                .into_iter()
                                .map(String::from)
                                .collect(),
                        )
                        .max_visible(3),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("Custom separator")
                .description(".separator(..) swaps the glyph between segments — arrows, dots, slashes.")
                .body(
                    column(children![
                        breadcrumb(vec!["Home".into(), "Docs".into(), "Getting started".into()])
                            .separator(lucide::ARROW_RIGHT),
                        gap_h(16.0),
                        breadcrumb(vec!["Home".into(), "Docs".into(), "Components".into()])
                            .separator(IconKind::Dot),
                        gap_h(16.0),
                        breadcrumb(vec!["Home".into(), "Docs".into(), "API".into()])
                            .separator(lucide::MINUS),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("Styled trail")
                .description("A Style sets the segment text (color, size — the active segment takes the color) and wraps the trail with box props.")
                .body(
                    column(children![
                        breadcrumb(vec!["Home".into(), "Docs".into(), "Components".into()])
                            .style(style().color(palette::blue::S600).font_size(15.0)),
                        gap_h(16.0),
                        breadcrumb(vec!["Home".into(), "Docs".into(), "Components".into()])
                            .style(
                                style()
                                    .background(theme().colors.card)
                                    .border(Border::new(theme().colors.border, 1.0))
                                    .radius_all(999.0)
                                    .padding_xy(10.0, 6.0),
                            ),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
        ])
}
