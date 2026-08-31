use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn grid_view() -> Element {
    screen("Grid View")
        .description("A virtualized grid — fixed columns × fixed row height, only visible rows built. Cells can SPAN columns and rows (the CSS-grid colspan/rowspan): the packing wraps around them.")
        .body(children![
            doc("600 cells, 4 columns")
                .description("Only the visible rows are built, even while scrolling fast.")
                .body(
                    Container::new()
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .height(260.0)
                        .child(GridView::builder(600, 4, 96.0, |i| {
                            let c = theme().colors;
                            Container::new()
                                .padding(EdgeInsets::all(6.0))
                                .child(
                                    Container::new()
                                        .decoration(BoxDecoration::new().color(c.secondary).radius(BorderRadius::all(theme().radius)))
                                        .alignment(Alignment::CENTER)
                                        .child(text(format!("#{i}")).size(14.0).color(c.foreground)),
                                )
                        })),
                ),
            doc("Spanning cells — colspan & rowspan")
                .description(".spans(|i| (cols, rows)) lets a cell occupy several grid cells, CSS-grid style: the layout packs around it. Here a dashboard: a 2×2 hero, a 2×1 banner, and singles filling around them.")
                .body(
                    Container::new()
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .height(340.0)
                        .child(GridView::builder(12, 4, 84.0, |i| {
                            tile(i, ["Overview", "Revenue", "Users", "Latency", "Storage", "Bandwidth", "Uptime", "Errors", "Region", "Plans", "Logs", "Backups"][i])
                        })
                        .spans(|i| match i {
                            0 => (2, 2), // hero tile
                            6 => (2, 1), // wide banner
                            _ => (1, 1),
                        })),
                ),
            doc("Fewer columns, taller cells")
                .description("Columns and row extent are yours — a photo-grid feel: 3 columns of square tiles.")
                .body(
                    Container::new()
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .height(280.0)
                        .child(GridView::builder(30, 3, 100.0, |i| {
                            Container::new()
                                .padding(EdgeInsets::all(5.0))
                                .child(
                                    Container::new()
                                        .decoration(
                                            BoxDecoration::new()
                                                .gradient(Gradient::Linear {
                                                    begin: Alignment::TOP_LEFT,
                                                    end: Alignment::BOTTOM_RIGHT,
                                                    colors: vec![
                                                        palette::violet::S600,
                                                        palette::blue::S600,
                                                    ],
                                                })
                                                .radius(BorderRadius::all(theme().radius)),
                                        )
                                        .alignment(Alignment::CENTER)
                                        .child(text(format!("{i}")).size(14.0).color(theme().colors.muted_foreground)),
                                )
                        })),
                ),
        ])
}

fn tile(i: usize, label: &str) -> impl IntoWidget {
    let colors = [palette::violet::S600, palette::blue::S600, palette::emerald::S600, palette::amber::S500];
    let tint = colors[i % colors.len()];
    Container::new()
        .padding(EdgeInsets::all(6.0))
        .child(
            Container::new()
                .decoration(BoxDecoration::new().color(tint).radius(BorderRadius::all(theme().radius)))
                .alignment(Alignment::CENTER)
                .child(text(label.to_string()).size(14.0).semibold().color(palette::WHITE)),
        )
}
