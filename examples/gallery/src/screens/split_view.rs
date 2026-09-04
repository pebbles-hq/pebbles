use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn split_views() -> Element {
    screen("Split View")
        .description("Two titled panes sharing a surface — split_view(panel(..), panel(..)) with a settable ratio, horizontal or vertical.")
        .body(children![
            doc("Explorer + editor")
                .description("The app-shell split: a tree on the left, an editor on the right.")
                .body(
                    container()
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .height(220.0)
                        .child(
                            split_view(
                                panel("EXPLORER", tree_view(vec![
                                    tree_node("app").icon(IconKind::Dot),
                                    tree_node("lib").icon(IconKind::Dot),
                                    tree_node("README.md").icon(IconKind::Dot),
                                ])),
                                panel("EDITOR", body("// Two resizable panes.")),
                            )
                            .ratio(0.4),
                        ),
                ),
            doc("Ratios")
                .description(".ratio(f64) sets the first pane's share — narrow, balanced, wide.")
                .body(
                    column(children![
                        split_sample(0.3, "Narrow", "Wide"),
                        gap_h(10.0),
                        split_sample(0.5, "Half", "Half"),
                        gap_h(10.0),
                        split_sample(0.7, "Wide", "Narrow"),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("Vertical")
                .description("split_view::vertical(..) stacks the panes — a console under an editor.")
                .body(
                    container()
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .height(240.0)
                        .child(
                            SplitView::vertical(
                                panel("EDITOR", body("// The top pane.")),
                                panel("TERMINAL", body("$ cargo run")),
                            )
                            .ratio(0.6),
                        ),
                ),
        ])
}

fn split_sample(ratio: f64, first: &str, second: &str) -> impl IntoWidget {
    container()
        .width(460.0)
        .decoration(
            BoxDecoration::new()
                .border(Border::new(theme().colors.border, 1.0))
                .radius(BorderRadius::all(theme().radius)),
        )
        .height(120.0)
        .child(split_view(panel(first, body(first)), panel(second, body(second))).ratio(ratio))
}
