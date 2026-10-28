use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn breadcrumbs() -> Element {
    screen("Breadcrumb").body(children![
        doc("Basic").body(breadcrumb(vec!["Home".into(), "Projects".into(), "Pebbles".into()]),),
        doc("Ellipsis — max_visible").body(
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
        doc("Custom separator").body(
            column(children![
                breadcrumb(vec!["Home".into(), "Docs".into(), "Getting started".into()])
                    .separator(tabler::ARROW_RIGHT),
                gap_h(16.0),
                breadcrumb(vec!["Home".into(), "Docs".into(), "Components".into()]).separator(IconKind::Dot),
                gap_h(16.0),
                breadcrumb(vec!["Home".into(), "Docs".into(), "API".into()]).separator(tabler::MINUS),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        ),
        doc("Styled trail").body(
            column(children![
                breadcrumb(vec!["Home".into(), "Docs".into(), "Components".into()])
                    .style(style().color(palette::blue::S600).font_size(15.0)),
                gap_h(16.0),
                breadcrumb(vec!["Home".into(), "Docs".into(), "Components".into()]).style(
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
