use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn paginations() -> Element {
    let page = create_signal(5usize);

    screen("Pagination").body(children![
        doc("The full control").body(
            column(children![
                pagination(page.get(), 20).variant(PaginationVariant::Numbers).on_page(move |p| page.set(p)),
                gap_h(8.0),
                muted(format!("page signal: {}", page.get())),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        ),
        doc(".edges(false) — no first/last jump").body(
            column(children![
                pagination(page.get(), 20)
                    .variant(PaginationVariant::Numbers)
                    .edges(false)
                    .on_page(move |p| page.set(p)),
                gap_h(16.0),
                muted("short range (2 of 5)"),
                gap_h(8.0),
                pagination(2, 5).variant(PaginationVariant::Numbers).on_page(|_| {}),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        ),
        doc("max_buttons").body(
            column(children![
                text("max_buttons(9)").size(12.5).color(theme().colors.muted_foreground),
                gap_h(8.0),
                pagination(10, 30).variant(PaginationVariant::Numbers).max_buttons(9).on_page(|_| {}),
                gap_h(16.0),
                text("max_buttons(5)").size(12.5).color(theme().colors.muted_foreground),
                gap_h(8.0),
                pagination(10, 30).variant(PaginationVariant::Numbers).max_buttons(5).on_page(|_| {}),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        ),
        doc("Simple & Arrows").body(
            column(children![
                text("Simple").size(12.5).color(theme().colors.muted_foreground),
                gap_h(8.0),
                pagination(page.get(), 20).variant(PaginationVariant::Simple).on_page(move |p| page.set(p)),
                gap_h(16.0),
                text("Arrows").size(12.5).color(theme().colors.muted_foreground),
                gap_h(8.0),
                pagination(page.get(), 20).variant(PaginationVariant::Arrows).on_page(move |p| page.set(p)),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        ),
        doc("Styled surface").body(
            column(children![
                pagination(page.get(), 20)
                    .variant(PaginationVariant::Numbers)
                    .style(
                        style()
                            .background(theme().colors.card)
                            .radius_all(theme().radius)
                            .border(Border::new(theme().colors.border, 1.0))
                            .padding_xy(6.0, 4.0),
                    )
                    .on_page(move |p| page.set(p)),
                gap_h(16.0),
                pagination(page.get(), 20)
                    .variant(PaginationVariant::Arrows)
                    .style(
                        style().background(theme().colors.secondary).radius_all(999.0).padding_xy(10.0, 4.0),
                    )
                    .on_page(move |p| page.set(p)),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        ),
        doc("In a table footer").body(
            table(vec!["Name".into(), "Role".into()])
                .row(vec!["Reyco", "Lead"])
                .row(vec!["Andres", "Engineer"])
                .row(vec!["Joseph", "Engineer"])
                .footer(padding(
                    EdgeInsets::symmetric(12.0, 8.0),
                    row(children![
                        muted("3 of 20 rows"),
                        spacer(),
                        pagination(page.get(), 20)
                            .variant(PaginationVariant::Numbers)
                            .on_page(move |p| page.set(p)),
                    ]),
                ),)
                .style(style().border(Border::new(theme().colors.border, 1.0))),
        ),
    ])
}
