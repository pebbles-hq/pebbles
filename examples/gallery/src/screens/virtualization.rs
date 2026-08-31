use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn virtualization() -> Element {
    let list_ctrl = use_scroll_controller();

    screen("Virtualization")
        .description("ListView and GridView build only the visible rows/cells — 5,000 rows and 600 cells stay smooth. Drive them with a ScrollController or the scrollbar.")
        .body(children![
            doc("ListView — 5,000 rows")
                .description("Only the visible rows are built. Smooth wheel, keyboard (PageDn/Home/End), the scrollbar, or a ScrollController.")
                .body(
                    column(children![
                        row(children![
                            button("Top").variant(ButtonVariant::Secondary).size(ButtonSize::Sm)
                                .on_pressed(move || list_ctrl.animate_to(0.0)),
                            gap_w(8.0),
                            button("Jump to #2500").variant(ButtonVariant::Secondary).size(ButtonSize::Sm)
                                .on_pressed(move || list_ctrl.scroll_to_index(2500, 44.0)),
                            gap_w(8.0),
                            button("Bottom").variant(ButtonVariant::Secondary).size(ButtonSize::Sm)
                                .on_pressed(move || list_ctrl.animate_to(5000.0 * 44.0)),
                        ])
                        .main_axis_size(MainAxisSize::Min),
                        gap_h(10.0),
                        Container::new()
                            .decoration(
                                BoxDecoration::new()
                                    .border(Border::new(theme().colors.border, 1.0))
                                    .radius(BorderRadius::all(theme().radius)),
                            )
                            .height(300.0)
                            .child(ListView::builder(5000, 44.0, |i| {
                                let c = theme().colors;
                                Container::new()
                                    .height(44.0)
                                    .padding(EdgeInsets::symmetric(14.0, 0.0))
                                    .alignment(Alignment::CENTER_LEFT)
                                    .decoration(BoxDecoration::new().border(Border::new(c.border, 0.5)))
                                    .child(row(children![
                                        icon(IconKind::Dot).size(16.0).color(c.muted_foreground),
                                        gap_w(10.0),
                                        text(format!("Row {i}")).size(14.0).color(c.foreground),
                                        spacer(),
                                        muted(format!("#{i}")),
                                    ]))
                            })
                            .controller(list_ctrl)),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("GridView — 600 cells, 4 columns")
                .description("Only the visible rows are built, even while scrolling fast.")
                .body(
                    Container::new()
                        .decoration(
                            BoxDecoration::new()
                                .border(Border::new(theme().colors.border, 1.0))
                                .radius(BorderRadius::all(theme().radius)),
                        )
                        .height(260.0)
                        .child(GridView::builder(600, 4, 96.0, |i| {
                            let c = theme().colors;
                            Container::new()
                                .padding(EdgeInsets::all(6.0))
                                .child(
                                    Container::new()
                                        .decoration(
                                            BoxDecoration::new()
                                                .color(c.secondary)
                                                .radius(BorderRadius::all(theme().radius)),
                                        )
                                        .alignment(Alignment::CENTER)
                                        .child(text(format!("#{i}")).size(14.0).color(c.foreground)),
                                )
                        })),
                ),
        ])
}
