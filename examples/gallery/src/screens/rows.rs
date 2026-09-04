use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

/// A colored swatch used as a flex child throughout the layout screens.
fn chip(color: Color, w: f64, h: f64) -> Container {
    // A zero dimension means "fill": the container omits that SizedBox, and a
    // childless decorated container expands to its constraints (Flutter parity).
    let mut c = container().decoration(BoxDecoration::new().color(color).radius(BorderRadius::all(6.0)));
    if w > 0.0 {
        c = c.width(w);
    }
    if h > 0.0 {
        c = c.height(h);
    }
    c
}

/// A bordered stage that constrains its child, so alignment effects are visible.
fn stage(h: f64, child: impl IntoWidget) -> Container {
    container()
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

pub fn rows() -> Element {
    screen("Row")
        .description(
            "Horizontal flex in Flutter-parity detail: six main-axis alignments, cross-axis alignment incl. baseline and stretch, spacing, shrink-wrap vs fill, flex factors — then the real-world patterns (toolbar, card header) they compose into.",
        )
        .body(children![
            any_count(),
            many_items(),
            main_axis(),
            cross_axis(),
            baseline(),
            spacing(),
            axis_size(),
            expanded_flexible(),
            patterns(),
        ])
}

fn any_count() -> impl IntoWidget {
    let count = create_signal(3usize);
    let n = count.get();
    let colors =
        [palette::BLUE, palette::GREEN, palette::AMBER, palette::PURPLE, palette::TEAL, palette::INDIGO];
    doc("Any number of children")
        .description("Rows take as many children as you like — drag the slider to grow the row live. Shown twice: SpaceBetween (children pinned to the edges, gaps flex) and Start with fixed spacing.")
        .body(
            column(children![
                row(children![
                    slider(320.0).min(2.0).max(24.0).step(1.0).value(n as f64).on_changed(move |v| count.set(v[0] as usize)),
                    gap_w(10.0),
                    muted(format!("{n} items")).size(12.0),
                ])
                .main_axis_size(MainAxisSize::Min),
                gap_h(8.0),
                stage(
                    40.0,
                    row({
                        let mut items: Vec<AnyWidget> = Vec::new();
                        for i in 0..n {
                            items.push(chip(colors[i % colors.len()], 26.0, 20.0).into_widget());
                        }
                        items
                    })
                    .main_axis_alignment(MainAxisAlignment::SpaceBetween),
                )
                .into_widget(),
                gap_h(8.0),
                stage(
                    40.0,
                    row({
                        let mut items: Vec<AnyWidget> = Vec::new();
                        for i in 0..n {
                            items.push(chip(colors[i % colors.len()], 26.0, 20.0).into_widget());
                        }
                        items
                    })
                    .spacing(4.0),
                )
                .into_widget(),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min),
        )
}

fn many_items() -> impl IntoWidget {
    let colors =
        [palette::BLUE, palette::GREEN, palette::AMBER, palette::PURPLE, palette::TEAL, palette::INDIGO];
    doc("Many items, live")
        .description("A static look at 16 children in one row: top with .spacing(4) so every chip is visible, bottom with SpaceBetween so the edges pin and the gaps flex. No item limit — only your width.")
        .body(
            column(children![
                stage(
                    40.0,
                    row({
                        let mut items: Vec<AnyWidget> = Vec::new();
                        for i in 0..16 {
                            items.push(chip(colors[i % colors.len()], 24.0, 20.0).into_widget());
                        }
                        items
                    })
                    .spacing(4.0),
                )
                .into_widget(),
                gap_h(8.0),
                stage(
                    40.0,
                    row({
                        let mut items: Vec<AnyWidget> = Vec::new();
                        for i in 0..16 {
                            items.push(chip(colors[i % colors.len()], 24.0, 20.0).into_widget());
                        }
                        items
                    })
                    .main_axis_alignment(MainAxisAlignment::SpaceBetween),
                )
                .into_widget(),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min),
        )
}

fn main_axis() -> impl IntoWidget {
    let (a, b, c) = (palette3()[0], palette3()[1], palette3()[2]);
    let rows: [(MainAxisAlignment, &str); 6] = [
        (MainAxisAlignment::Start, "Start"),
        (MainAxisAlignment::Center, "Center"),
        (MainAxisAlignment::End, "End"),
        (MainAxisAlignment::SpaceBetween, "SpaceBetween"),
        (MainAxisAlignment::SpaceAround, "SpaceAround"),
        (MainAxisAlignment::SpaceEvenly, "SpaceEvenly"),
    ];
    doc("Main axis alignment")
        .description("How children distribute along the horizontal axis: packed left, centered, packed right, or the three space modes — between (first/last flush), around (half gaps at the ends) and evenly (equal gaps everywhere).")
        .body(
            column({
                let mut items: Vec<AnyWidget> = Vec::new();
                for (alignment, label) in rows {
                    items.push(
                        column(children![
                            muted(label.to_string()).size(11.5),
                            gap_h(4.0),
                            stage(
                                48.0,
                                row(children![
                                    chip(a, 56.0, 28.0),
                                    chip(b, 56.0, 28.0),
                                    chip(c, 56.0, 28.0),
                                ])
                                .main_axis_alignment(alignment),
                            )
                            .into_widget(),
                        ])
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
}

fn cross_axis() -> impl IntoWidget {
    let (a, b, c) = (palette3()[0], palette3()[1], palette3()[2]);
    let rows: [(CrossAxisAlignment, &str); 4] = [
        (CrossAxisAlignment::Start, "Start"),
        (CrossAxisAlignment::Center, "Center"),
        (CrossAxisAlignment::End, "End"),
        (CrossAxisAlignment::Stretch, "Stretch"),
    ];
    doc("Cross axis alignment")
        .description("Where children sit vertically. Start/End honor the vertical direction, Center floats them mid-height, and Stretch forces every child to the row's full height — for Start/Center/End the chips keep their intrinsic heights, and for Stretch they drop the explicit height so the stretch is visible (explicit sizes always win, exactly like Flutter).")
        .body(
            column({
                let mut items: Vec<AnyWidget> = Vec::new();
                for (alignment, label) in rows {
                    let stretched = matches!(alignment, CrossAxisAlignment::Stretch);
                    items.push(
                        column(children![
                            muted(label.to_string()).size(11.5),
                            gap_h(4.0),
                            stage(
                                76.0,
                                row(children![
                                    chip(a, 56.0, if stretched { 0.0 } else { 20.0 }),
                                    chip(b, 56.0, if stretched { 0.0 } else { 40.0 }),
                                    chip(c, 56.0, if stretched { 0.0 } else { 60.0 }),
                                ])
                                .cross_axis_alignment(alignment),
                            )
                            .into_widget(),
                        ])
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
}

fn baseline() -> impl IntoWidget {
    doc("Baseline")
        .description("CrossAxisAlignment::Baseline aligns the text baselines of mixed-size children — the typographically correct way to mix sizes on one line (falls back to Start when no baseline exists).")
        .body(
            stage(
                64.0,
                row(children![
                    text("big").size(30.0),
                    text("medium").size(20.0),
                    text("small").size(12.0),
                    chip(palette::PURPLE, 48.0, 40.0),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Baseline)
                .spacing(12.0),
            ),
        )
}

fn spacing() -> impl IntoWidget {
    let (a, b, c) = (palette3()[0], palette3()[1], palette3()[2]);
    doc("Spacing")
        .description(".spacing(n) reserves a fixed gap between children (Flutter's Flex.spacing) — cleaner than padding hacks for lists of controls.")
        .body(
            column({
                let mut items: Vec<AnyWidget> = Vec::new();
                for s in [0.0, 8.0, 16.0] {
                    items.push(
                        column(children![
                            muted(format!("spacing({s})")).size(11.5),
                            gap_h(4.0),
                            stage(
                                44.0,
                                row(children![
                                    chip(a, 40.0, 24.0),
                                    chip(b, 40.0, 24.0),
                                    chip(c, 40.0, 24.0),
                                ])
                                .spacing(s),
                            )
                            .into_widget(),
                        ])
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
}

fn axis_size() -> impl IntoWidget {
    let (a, b, c) = (palette3()[0], palette3()[1], palette3()[2]);
    doc("Main axis size")
        .description("MainAxisSize::Min shrink-wraps the row to its children (the border hugs the chips); MainAxisSize::Max stretches it to fill the available width — the default, so SpaceBetween-style layouts have room to work.")
        .body(
            column(children![
                column(children![
                    muted("MainAxisSize::Min — shrink-wrapped").size(11.5),
                    gap_h(4.0),
                    container()
                        .padding(EdgeInsets::all(6.0))
                        .decoration(
                            BoxDecoration::new()
                                .border(Border::new(theme().colors.border, 1.0))
                                .radius(BorderRadius::all(theme().radius)),
                        )
                        .child(
                            row(children![
                                chip(a, 56.0, 28.0),
                                chip(b, 56.0, 28.0),
                                chip(c, 56.0, 28.0),
                            ])
                            .main_axis_size(MainAxisSize::Min)
                            .spacing(8.0),
                        )
                        .into_widget(),
                ])
                .main_axis_size(MainAxisSize::Min),
                gap_h(10.0),
                column(children![
                    muted("MainAxisSize::Max — fills the stage").size(11.5),
                    gap_h(4.0),
                    stage(
                        44.0,
                        row(children![
                            chip(a, 56.0, 28.0),
                            chip(b, 56.0, 28.0),
                            chip(c, 56.0, 28.0),
                        ])
                        .spacing(8.0),
                    )
                    .into_widget(),
                ])
                .main_axis_size(MainAxisSize::Min),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min),
        )
}

fn expanded_flexible() -> impl IntoWidget {
    let (a, b, c) = (palette3()[0], palette3()[1], palette3()[2]);
    doc("Expanded & Flexible")
        .description("Flex factors split the leftover space proportionally. Expanded (FlexFit::Tight) always fills its share; Flexible (FlexFit::Loose) only grows up to its share — a narrow child keeps its intrinsic size and leaves the rest empty.")
        .body(
            column(children![
                column(children![
                    muted("Expanded flex 2 : 1 : 1").size(11.5),
                    gap_h(4.0),
                    stage(
                        44.0,
                        row(children![
                            expanded(chip(a, 0.0, 0.0)).flex(2),
                            gap_w(8.0),
                            expanded(chip(b, 0.0, 0.0)).flex(1),
                            gap_w(8.0),
                            expanded(chip(c, 0.0, 0.0)).flex(1),
                        ]),
                    )
                    .into_widget(),
                ])
                .main_axis_size(MainAxisSize::Min),
                gap_h(10.0),
                column(children![
                    muted("Flexible (loose) keeps its intrinsic 60px — the rest stays empty").size(11.5),
                    gap_h(4.0),
                    stage(
                        44.0,
                        row(children![
                            chip(a, 100.0, 28.0),
                            gap_w(8.0),
                            flexible(chip(b, 60.0, 28.0)),
                        ]),
                    )
                    .into_widget(),
                ])
                .main_axis_size(MainAxisSize::Min),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min),
        )
}

fn patterns() -> impl IntoWidget {
    let th = theme();
    doc("Real-world patterns")
        .description("The pieces compose into everyday app chrome: a toolbar with spacer() pushing trailing actions right, and a card header with a trailing badge.")
        .body(
            column(children![
                container()
                    .padding(EdgeInsets::symmetric(10.0, 12.0))
                    .decoration(
                        BoxDecoration::new()
                            .color(th.colors.card)
                            .border(Border::new(th.colors.border, 1.0))
                            .radius(BorderRadius::all(th.radius)),
                    )
                    .child(
                        row(children![
                            icon(IconKind::Menu).size(16.0).color(th.colors.foreground),
                            gap_w(10.0),
                            text("Inbox").semibold(),
                            spacer(),
                            button("New").leading(IconKind::Plus).on_click(|| {}),
                            gap_w(8.0),
                            icon(IconKind::User).size(16.0).color(th.colors.muted_foreground),
                        ]),
                    )
                    .into_widget(),
                gap_h(10.0),
                container()
                    .padding(EdgeInsets::all(12.0))
                    .decoration(
                        BoxDecoration::new()
                            .color(th.colors.card)
                            .border(Border::new(th.colors.border, 1.0))
                            .radius(BorderRadius::all(th.radius)),
                    )
                    .child(
                        row(children![
                            text("Q3 report").size(18.0).semibold(),
                            gap_w(8.0),
                            badge("new").variant(BadgeVariant::Secondary),
                            spacer(),
                            muted("updated 2m ago").size(12.0),
                        ]),
                    )
                    .into_widget(),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min),
        )
}
