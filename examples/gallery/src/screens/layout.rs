use pebbles::prelude::*;

use crate::ui::{gap_w, screen, section};

pub fn layout() -> Element {
    let swatch = |color: Color, w: f64| {
        Container::new()
            .decoration(BoxDecoration::new().color(color).radius(BorderRadius::all(6.0)))
            .width(w)
            .height(40.0)
    };

    screen(
        "Layout",
        "Row, Column, Stack, Wrap, Expanded, AspectRatio — the box protocol.",
        children![
            section(
                "ROW + EXPANDED",
                row(children![
                    Expanded::new(swatch(palette::BLUE, 0.0)).flex(2),
                    gap_w(8.0),
                    Expanded::new(swatch(palette::GREEN, 0.0)).flex(1),
                    gap_w(8.0),
                    Expanded::new(swatch(palette::AMBER, 0.0)).flex(1),
                ]),
            ),
            section(
                "WRAP (reflows to width)",
                wrap(
                    [
                        "design", "rust", "vello", "gpu", "widgets", "flutter", "desktop", "shadcn",
                        "layout", "reactive", "pebbles", "gravel",
                    ]
                    .into_iter()
                    .map(|t| badge(t).variant(BadgeVariant::Secondary))
                    .collect::<Vec<_>>(),
                )
                .spacing(8.0)
                .run_spacing(8.0),
            ),
            section(
                "STACK + POSITIONED",
                Container::new()
                    .decoration(BoxDecoration::new().color(theme().colors.muted).radius(BorderRadius::all(8.0)))
                    .height(120.0)
                    .child(stack(children![
                        Positioned::new(swatch(palette::PURPLE, 90.0)).left(12.0).top(12.0),
                        Positioned::new(swatch(palette::TEAL, 90.0)).right(12.0).bottom(12.0),
                        Positioned::new(badge("center")).left(150.0).top(48.0),
                    ])),
            ),
            section(
                "ASPECT RATIO (16:9)",
                SizedBox::exact(
                    260.0,
                    150.0,
                    aspect_ratio(
                        16.0 / 9.0,
                        Container::new().decoration(
                            BoxDecoration::new().color(palette::INDIGO).radius(BorderRadius::all(8.0)),
                        ),
                    ),
                ),
            ),
            section(
                "SCROLL AREA (bounded, always-on scrollbar)",
                scroll_area(
                    column((1..=25).map(|i| text(format!("Line {i}"))).collect::<Vec<_>>())
                        .start()
                        .min()
                        .spacing(6.0),
                )
                .width(260.0)
                .height(160.0),
            ),
        ],
    )
}
