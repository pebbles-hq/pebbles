use pebbles::prelude::*;

use crate::ui::{gap_h, screen};

pub fn icons() -> impl IntoWidget {
    let c = theme().colors;
    let all = [
        (IconKind::Check, "Check"),
        (IconKind::Close, "Close"),
        (IconKind::Plus, "Plus"),
        (IconKind::Minus, "Minus"),
        (IconKind::ChevronDown, "ChevronDown"),
        (IconKind::ChevronRight, "ChevronRight"),
        (IconKind::Search, "Search"),
        (IconKind::Star, "Star"),
        (IconKind::Info, "Info"),
        (IconKind::Warning, "Warning"),
        (IconKind::Menu, "Menu"),
        (IconKind::ArrowRight, "ArrowRight"),
        (IconKind::Dot, "Dot"),
        (IconKind::Circle, "Circle"),
    ];
    screen(
        "Icons",
        "The built-in Lucide-style vector icon set.",
        children![
            wrap(all.into_iter().map(move |(k, name)| {
                Container::new()
                    .decoration(BoxDecoration::new().color(c.card).border(Border::new(c.border, 1.0)).radius(BorderRadius::all(8.0)))
                    .padding(EdgeInsets::all(12.0))
                    .width(120.0)
                    .child(
                        column(children![icon(k).size(24.0).color(c.foreground), gap_h(8.0), muted(name)])
                            .cross_axis_alignment(CrossAxisAlignment::Center)
                            .main_axis_min(),
                    )
            }))
            .spacing(12.0)
            .run_spacing(12.0),
        ],
    )
}
