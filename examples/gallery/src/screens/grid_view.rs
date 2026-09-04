use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn grid_view() -> Element {
    screen("Grid View")
        .description("A virtualized grid — fixed columns × fixed row height, only visible rows built. Cells span columns AND rows (CSS-grid colspan/rowspan); spacing, width-derived aspect ratios, and responsive max-extent columns keep it flexible.")
        .body(children![
            doc("600 cells, 4 columns")
                .description("Only the visible rows are built, even while scrolling fast.")
                .body(
                    container()
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .height(260.0)
                        .child(GridView::builder(600, 4, 96.0, |i| {
                            let c = theme().colors;
                            container()
                                .padding(EdgeInsets::all(6.0))
                                .child(
                                    container()
                                        .decoration(BoxDecoration::new().color(c.secondary).radius(BorderRadius::all(theme().radius)))
                                        .alignment(Alignment::CENTER)
                                        .child(text(format!("#{i}")).size(14.0).color(c.foreground)),
                                )
                        })),
                ),
            doc("Spanning cells — colspan & rowspan")
                .description(".spans(|i| (cols, rows)) lets a cell occupy several grid cells, CSS-grid style — the layout packs around it. Here a magazine layout: a 2×2 hero, a 1×2 TALL tile, a 2×1 banner, and singles filling around them.")
                .body(
                    container()
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .height(360.0)
                        .child(GridView::builder(14, 4, 84.0, |i| {
                            tile(i, ["Hero", "A", "B", "Tall", "C", "D", "Wide", "E", "F", "G", "H", "I", "J", "K"][i])
                        })
                        .spans(|i| match i {
                            0 => (2, 2), // hero: 2 columns × 2 rows
                            3 => (1, 2), // tall: 1 column × 2 rows
                            6 => (2, 1), // wide: 2 columns × 1 row
                            _ => (1, 1),
                        })),
                ),
            doc("Spacing")
                .description(".spacing(px) gaps rows AND columns — the tiled look without self-padding.")
                .body(
                    container()
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .height(240.0)
                        .child(GridView::builder(24, 3, 72.0, |i| {
                            container()
                                .decoration(BoxDecoration::new().color(theme().colors.secondary).radius(BorderRadius::all(theme().radius)))
                                .alignment(Alignment::CENTER)
                                .child(text(format!("{i}")).size(13.0))
                        })
                        .spacing(8.0)),
                ),
            doc("Aspect ratio")
                .description(".aspect_ratio(1.0) derives the row height from the cell width — square cells at ANY width (Flutter's childAspectRatio).")
                .body(
                    container()
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .height(260.0)
                        .child(GridView::builder(20, 4, 80.0, |i| {
                            container()
                                .decoration(BoxDecoration::new().color(palette::sky::S500).radius(BorderRadius::all(theme().radius)))
                                .alignment(Alignment::CENTER)
                                .child(text(format!("{i}")).size(13.0).color(palette::WHITE))
                        })
                        .aspect_ratio(1.0)),
                ),
            doc("Responsive columns")
                .description(".max_extent(px) derives the column count from the available width — resize the window and the grid re-flows (Flutter's maxCrossAxisExtent).")
                .body(
                    container()
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .height(240.0)
                        .child(GridView::builder(40, 4, 64.0, |i| {
                            container()
                                .decoration(BoxDecoration::new().color(palette::emerald::S600).radius(BorderRadius::all(theme().radius)))
                                .alignment(Alignment::CENTER)
                                .child(text(format!("{i}")).size(13.0).color(palette::WHITE))
                        })
                        .max_extent(140.0)
                        .spacing(6.0)),
                ),
            doc("Reversed & padded")
                .description(".reverse() stacks rows from the bottom; .padding(EdgeInsets) scrolls with the content. Combine with spacing for a breathing layout.")
                .body(
                    container()
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .height(240.0)
                        .child(GridView::builder(16, 3, 72.0, |i| {
                            container()
                                .decoration(BoxDecoration::new().color(palette::amber::S500).radius(BorderRadius::all(theme().radius)))
                                .alignment(Alignment::CENTER)
                                .child(text(format!("{i}")).size(13.0))
                        })
                        .spacing(6.0)
                        .padding(EdgeInsets::all(10.0))
                        .reverse()),
                ),
        ])
}

fn tile(i: usize, label: &str) -> impl IntoWidget {
    let colors = [
        palette::violet::S600,
        palette::blue::S600,
        palette::emerald::S600,
        palette::amber::S500,
        palette::rose::S600,
    ];
    let tint = colors[i % colors.len()];
    container().padding(EdgeInsets::all(6.0)).child(
        container()
            .decoration(BoxDecoration::new().color(tint).radius(BorderRadius::all(theme().radius)))
            .alignment(Alignment::CENTER)
            .child(text(label.to_string()).size(14.0).semibold().color(palette::WHITE)),
    )
}
