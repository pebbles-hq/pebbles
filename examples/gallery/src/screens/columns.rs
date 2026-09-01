use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

fn chip(color: Color, w: f64, h: f64) -> Container {
    Container::new()
        .width(w)
        .height(h)
        .decoration(BoxDecoration::new().color(color).radius(BorderRadius::all(6.0)))
}

fn stage(h: f64, child: impl IntoWidget) -> Container {
    Container::new()
        .height(h)
        .padding(EdgeInsets::all(6.0))
        .decoration(
            BoxDecoration::new()
                .border(Border::new(theme().colors.border, 1.0))
                .radius(BorderRadius::all(theme().radius)),
        )
        .child(child)
}

fn palette3() -> [Color; 3] {
    [palette::BLUE, palette::GREEN, palette::AMBER]
}

pub fn columns() -> Element {
    screen("Column")
        .description(
            "Vertical flex, same contract as Row: six main-axis alignments in a fixed-height stage, cross-axis placement with stretch, spacing, shrink-wrap vs fill, vertical flex factors — and the chat-panel pattern they compose into.",
        )
        .body(children![
            main_axis(),
            cross_axis(),
            spacing(),
            axis_size(),
            expanded(),
            patterns(),
        ])
}

fn main_axis() -> impl IntoWidget {
    let (a, b, c) = (palette3()[0], palette3()[1], palette3()[2]);
    let cols: [(MainAxisAlignment, &str); 6] = [
        (MainAxisAlignment::Start, "Start"),
        (MainAxisAlignment::Center, "Center"),
        (MainAxisAlignment::End, "End"),
        (MainAxisAlignment::SpaceBetween, "SpaceBetween"),
        (MainAxisAlignment::SpaceAround, "SpaceAround"),
        (MainAxisAlignment::SpaceEvenly, "SpaceEvenly"),
    ];
    doc("Main axis alignment")
        .description("Vertical distribution inside a fixed-height stage. Note SpaceBetween pins the first and last chips to the edges — the classic header/footer trick with Expanded or spacing in between.")
        .body(
            row({
                let mut items: Vec<AnyWidget> = Vec::new();
                for (alignment, label) in cols {
                    items.push(
                        column(children![
                            stage(
                                132.0,
                                column(children![
                                    chip(a, 56.0, 20.0),
                                    chip(b, 56.0, 20.0),
                                    chip(c, 56.0, 20.0),
                                ])
                                .main_axis_alignment(alignment),
                            )
                            .into_widget(),
                            gap_h(6.0),
                            muted(label.to_string()).size(11.0).into_widget(),
                        ])
                        .main_axis_size(MainAxisSize::Min)
                        .into_widget(),
                    );
                }
                items
            })
            .main_axis_size(MainAxisSize::Min)
            .spacing(12.0),
        )
}

fn cross_axis() -> impl IntoWidget {
    let (a, b, c) = (palette3()[0], palette3()[1], palette3()[2]);
    let cols: [(CrossAxisAlignment, &str); 4] = [
        (CrossAxisAlignment::Start, "Start"),
        (CrossAxisAlignment::Center, "Center"),
        (CrossAxisAlignment::End, "End"),
        (CrossAxisAlignment::Stretch, "Stretch"),
    ];
    doc("Cross axis alignment")
        .description("Horizontal placement of children with different widths: pinned left, centered, pinned right, or Stretch — every child forced to the column's full width (the default for form-like layouts).")
        .body(
            row({
                let mut items: Vec<AnyWidget> = Vec::new();
                for (alignment, label) in cols {
                    items.push(
                        column(children![
                            stage(
                                116.0,
                                column(children![
                                    chip(a, 40.0, 18.0),
                                    chip(b, 96.0, 18.0),
                                    chip(c, 64.0, 18.0),
                                ])
                                .cross_axis_alignment(alignment)
                                .spacing(8.0),
                            )
                            .into_widget(),
                            gap_h(6.0),
                            muted(label.to_string()).size(11.0).into_widget(),
                        ])
                        .main_axis_size(MainAxisSize::Min)
                        .into_widget(),
                    );
                }
                items
            })
            .main_axis_size(MainAxisSize::Min)
            .spacing(12.0),
        )
}

fn spacing() -> impl IntoWidget {
    let (a, b, c) = (palette3()[0], palette3()[1], palette3()[2]);
    doc("Spacing")
        .description(".spacing(n) on a Column reserves vertical gaps between children — the backbone of form and settings stacks.")
        .body(
            row({
                let mut items: Vec<AnyWidget> = Vec::new();
                for s in [0.0, 8.0, 16.0] {
                    items.push(
                        column(children![
                            stage(
                                92.0,
                                column(children![
                                    chip(a, 56.0, 16.0),
                                    chip(b, 56.0, 16.0),
                                    chip(c, 56.0, 16.0),
                                ])
                                .spacing(s),
                            )
                            .into_widget(),
                            gap_h(6.0),
                            muted(format!("spacing({s})")).size(11.0).into_widget(),
                        ])
                        .main_axis_size(MainAxisSize::Min)
                        .into_widget(),
                    );
                }
                items
            })
            .main_axis_size(MainAxisSize::Min)
            .spacing(12.0),
        )
}

fn axis_size() -> impl IntoWidget {
    let (a, b, c) = (palette3()[0], palette3()[1], palette3()[2]);
    doc("Main axis size")
        .description("MainAxisSize::Min shrink-wraps the column (border hugs the chips); MainAxisSize::Max fills the stage height — the mode that makes End / SpaceBetween positioning meaningful.")
        .body(
            row(children![
                column(children![
                    stage(
                        120.0,
                        column(children![
                            chip(a, 56.0, 20.0),
                            chip(b, 56.0, 20.0),
                            chip(c, 56.0, 20.0),
                        ])
                        .main_axis_size(MainAxisSize::Min)
                        .spacing(8.0),
                    )
                    .into_widget(),
                    gap_h(6.0),
                    muted("Min — shrink-wrapped").size(11.0).into_widget(),
                ])
                .main_axis_size(MainAxisSize::Min),
                gap_w(12.0),
                column(children![
                    stage(
                        120.0,
                        column(children![
                            chip(a, 56.0, 20.0),
                            chip(b, 56.0, 20.0),
                            chip(c, 56.0, 20.0),
                        ])
                        .spacing(8.0),
                    )
                    .into_widget(),
                    gap_h(6.0),
                    muted("Max — fills the stage").size(11.0).into_widget(),
                ])
                .main_axis_size(MainAxisSize::Min),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn expanded() -> impl IntoWidget {
    let (a, b, c) = (palette3()[0], palette3()[1], palette3()[2]);
    doc("Expanded")
        .description("Vertical flex factors split leftover height: header gets 2, body 1, footer 1 — the same flex math as Row, rotated.")
        .body(
            column(children![
                stage(
                    160.0,
                    column(children![
                        Expanded::new(chip(a, 0.0, 0.0)).flex(2),
                        gap_h(8.0),
                        Expanded::new(chip(b, 0.0, 0.0)).flex(1),
                        gap_h(8.0),
                        Expanded::new(chip(c, 0.0, 0.0)).flex(1),
                    ]),
                )
                .into_widget(),
                gap_h(6.0),
                muted("flex 2 : 1 : 1").size(11.0).into_widget(),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn patterns() -> impl IntoWidget {
    let th = theme();
    doc("Chat panel pattern")
        .description("A header, an Expanded scrollable body, and a footer — the canonical app-panel skeleton, built entirely from Column + Expanded.")
        .body(
            Container::new()
                .height(300.0)
                .decoration(
                    BoxDecoration::new()
                        .color(th.colors.card)
                        .border(Border::new(th.colors.border, 1.0))
                        .radius(BorderRadius::all(th.radius)),
                )
                .child(
                    column(children![
                        Container::new()
                            .padding(EdgeInsets::symmetric(12.0, 14.0))
                            .decoration(BoxDecoration::new().border(Border::only(BorderSide::NONE, BorderSide::NONE, BorderSide::new(th.colors.border, 1.0), BorderSide::NONE)))
                            .child(
                                row(children![
                                    icon(IconKind::User).size(15.0).color(th.colors.muted_foreground),
                                    gap_w(8.0),
                                    text("#general").semibold(),
                                    spacer(),
                                    icon(IconKind::Search).size(15.0).color(th.colors.muted_foreground),
                                ]),
                            )
                            .into_widget(),
                        Expanded::new(
                            scroll_area(
                                column({
                                    let mut items: Vec<AnyWidget> = Vec::new();
                                    for (i, author) in [
                                        (0, "ana"),
                                        (1, "you"),
                                        (2, "bob"),
                                        (3, "ana"),
                                        (4, "you"),
                                        (5, "bob"),
                                    ] {
                                        let mine = author == "you";
                                        let fg = if mine { palette::WHITE } else { th.colors.foreground };
                                        items.push(
                                            column(children![
                                                muted(if mine { "you" } else { author }).size(11.0),
                                                Container::new()
                                                    .padding(EdgeInsets::symmetric(6.0, 10.0))
                                                    .decoration(
                                                        BoxDecoration::new()
                                                            .color(if mine { th.colors.primary } else { th.colors.secondary })
                                                            .radius(BorderRadius::all(8.0)),
                                                    )
                                                    .child(text(format!("Message {i} from {author} — wraps to the bubble width.")).size(12.5).color(fg))
                                                    .into_widget(),
                                            ])
                                            .cross_axis_alignment(if mine { CrossAxisAlignment::End } else { CrossAxisAlignment::Start })
                                            .main_axis_size(MainAxisSize::Min)
                                            .into_widget(),
                                        );
                                    }
                                    items
                                })
                                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                                .main_axis_size(MainAxisSize::Min)
                                .spacing(10.0),
                            )
                            .padding(EdgeInsets::all(14.0)),
                        )
                        .into_widget(),
                        Container::new()
                            .padding(EdgeInsets::all(10.0))
                            .decoration(BoxDecoration::new().border(Border::only(BorderSide::new(th.colors.border, 1.0), BorderSide::NONE, BorderSide::NONE, BorderSide::NONE)))
                            .child(
                                row(children![
                                    text_field().placeholder("Type a message…"),
                                    gap_w(8.0),
                                    button("Send"),
                                ]),
                            )
                            .into_widget(),
                    ]),
                ),
        )
}
