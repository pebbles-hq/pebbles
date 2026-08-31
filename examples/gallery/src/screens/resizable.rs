use pebbles::prelude::*;

use crate::ui::{doc, screen};

fn pane(label: &str, color: Color) -> AnyWidget {
    Container::new()
        .decoration(BoxDecoration::new().color(color))
        .alignment(Alignment::CENTER)
        .child(center(
            text(label.to_string())
                .size(13.0)
                .semibold()
                .color(palette::WHITE),
        ))
        .into_widget()
}

/// Bound the group's cross-axis (height for horizontal, width handled inline).
fn region(height: f64, child: impl IntoWidget) -> impl IntoWidget {
    Container::new()
        .height(height)
        .decoration(
            BoxDecoration::new()
                .border(Border::new(theme().colors.border, 1.0))
                .radius(BorderRadius::all(theme().radius)),
        )
        .clip()
        .child(child)
}

pub fn resizables() -> Element {
    screen("Resizable")
        .description("Panels split by draggable handles (shadcn's Resizable). Drag a handle to grow one panel and shrink its neighbor; horizontal or vertical, and nestable.")
        .body(
        children![two(), three(), vertical(), nested()],
    )
}

fn two() -> impl IntoWidget {
    doc("Two panels")
        .description("Drag the handle to resize. Each panel has a minimum size it can't cross.")
        .body(region(
            190.0,
            resizable(vec![
                pane("Sidebar", palette::indigo::S600),
                pane("Content", palette::slate::S600),
            ])
            .length(560.0)
            .min(90.0),
        ))
}

fn three() -> impl IntoWidget {
    doc("Three panels")
        .description("Any number of panels, each handle resizing its two neighbors.")
        .body(region(
            190.0,
            resizable(vec![
                pane("Files", palette::emerald::S600),
                pane("Editor", palette::slate::S600),
                pane("Preview", palette::blue::S600),
            ])
            .length(560.0)
            .sizes(vec![150.0, 260.0, 150.0])
            .min(80.0),
        ))
}

fn vertical() -> impl IntoWidget {
    doc("Vertical")
        .description(
            "Stack the panels with .orientation(Axis::Vertical) — the handle runs horizontally.",
        )
        .body(
            Container::new().width(360.0).child(region(
                300.0,
                resizable(vec![
                    pane("Top", palette::rose::S600),
                    pane("Bottom", palette::slate::S600),
                ])
                .orientation(Axis::Vertical)
                .length(300.0)
                .min(60.0),
            )),
        )
}

fn nested() -> impl IntoWidget {
    let inner = resizable(vec![
        pane("Editor", palette::slate::S600),
        pane("Terminal", palette::zinc::S700),
    ])
    .orientation(Axis::Vertical)
    .length(190.0)
    .min(50.0)
    .into_widget();
    doc("Nested")
        .description("Panels can contain other resizable groups — an IDE-style layout.")
        .body(region(
            190.0,
            resizable(vec![pane("Files", palette::emerald::S600), inner])
                .length(560.0)
                .sizes(vec![160.0, 400.0])
                .min(80.0),
        ))
}
