use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn paginations() -> Element {
    let page = create_signal(5usize);

    screen("Pagination")
        .description("Page navigation — three designs, one component: numbered pills with ellipses, chevrons around a label, or the compact X / Y. Every change reports through .on_page(..).")
        .body(children![
            doc("Numbers — the shadcn classic")
                .description("Numbered pills, ellipses for long ranges, chevrons at the ends; the active page is filled. Chevrons disable at the bounds.")
                .body(
                    column(children![
                        pagination(page.get(), 20)
                            .variant(PaginationVariant::Numbers)
                            .on_page(move |p| page.set(p)),
                        gap_h(8.0),
                        muted(format!("page signal: {}", page.get())),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("Short ranges — no ellipses")
                .description("With few pages every pill shows; the window is exact.")
                .body(
                    pagination(2, 5)
                        .variant(PaginationVariant::Numbers)
                        .on_page(|_| {}),
                ),
            doc("max_buttons")
                .description(".max_buttons(n) sizes the pill window before it collapses to ellipses (default 7, minimum 5) — wide or tight.")
                .body(
                    column(children![
                        text("max_buttons(9)").size(12.5).color(theme().colors.muted_foreground),
                        gap_h(8.0),
                        pagination(10, 30)
                            .variant(PaginationVariant::Numbers)
                            .max_buttons(9)
                            .on_page(|_| {}),
                        gap_h(16.0),
                        text("max_buttons(5)").size(12.5).color(theme().colors.muted_foreground),
                        gap_h(8.0),
                        pagination(10, 30)
                            .variant(PaginationVariant::Numbers)
                            .max_buttons(5)
                            .on_page(|_| {}),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("Simple & Arrows")
                .description("The label variants — 'Page X of Y' and the compact 'X / Y'.")
                .body(
                    column(children![
                        text("Simple").size(12.5).color(theme().colors.muted_foreground),
                        gap_h(8.0),
                        pagination(page.get(), 20)
                            .variant(PaginationVariant::Simple)
                            .on_page(move |p| page.set(p)),
                        gap_h(16.0),
                        text("Arrows").size(12.5).color(theme().colors.muted_foreground),
                        gap_h(8.0),
                        pagination(page.get(), 20)
                            .variant(PaginationVariant::Arrows)
                            .on_page(move |p| page.set(p)),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("Styled surface")
                .description("A Style covers the control — card background, radius, border, padding — for a framed pagination bar.")
                .body(
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
                                style()
                                    .background(theme().colors.secondary)
                                    .radius_all(999.0)
                                    .padding_xy(10.0, 4.0),
                            )
                            .on_page(move |p| page.set(p)),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("In a table footer")
                .description("The classic placement: pagination inside a DataTable's .footer(..) slot with a rows summary.")
                .body(
                    table(vec!["Name".into(), "Role".into()])
                        .row(vec!["Reyco", "Lead"])
                        .row(vec!["Andres", "Engineer"])
                        .row(vec!["Joseph", "Engineer"])
                        .footer(
                            Padding::new(
                                EdgeInsets::symmetric(12.0, 8.0),
                                row(children![
                                    muted("3 of 20 rows"),
                                    spacer(),
                                    pagination(page.get(), 20)
                                        .variant(PaginationVariant::Numbers)
                                        .on_page(move |p| page.set(p)),
                                ]),
                            ),
                        )
                        .style(style().border(Border::new(theme().colors.border, 1.0))),
                ),
        ])
}
