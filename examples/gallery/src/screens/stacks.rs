use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

fn chip(color: Color, w: f64, h: f64) -> Container {
    // A zero dimension means "fill": the container omits that SizedBox, and a
    // childless decorated container expands to its constraints (Flutter parity).
    let mut c = container()
        .decoration(BoxDecoration::new().color(color).radius(BorderRadius::all(6.0)));
    if w > 0.0 {
        c = c.width(w);
    }
    if h > 0.0 {
        c = c.height(h);
    }
    c
}

fn stage(w: f64, h: f64, child: impl IntoWidget) -> Container {
    container()
        .width(w)
        .height(h)
        .padding(EdgeInsets::all(6.0))
        .decoration(
            BoxDecoration::new()
                .border(Border::new(theme().colors.border, 1.0))
                .radius(BorderRadius::all(theme().radius)),
        )
        .child(child)
}

pub fn stacks() -> Element {
    screen("Stack")
        .description(
            "Overlapping children with Flutter's Stack contract: nine alignment slots, exact Positioned placement (edges, spans, fill), loose vs expand fit, z-order, and the real-world overlays — status dots, hero banners, ribbons.",
        )
        .body(children![
            alignment_grid(),
            positioned_section(),
            fit_and_zorder(),
            real_world(),
        ])
}

fn alignment_grid() -> impl IntoWidget {
    let slots: [(Alignment, &str); 9] = [
        (Alignment::TOP_LEFT, "topLeft"),
        (Alignment::TOP_CENTER, "topCenter"),
        (Alignment::TOP_RIGHT, "topRight"),
        (Alignment::CENTER_LEFT, "centerLeft"),
        (Alignment::CENTER, "center"),
        (Alignment::CENTER_RIGHT, "centerRight"),
        (Alignment::BOTTOM_LEFT, "bottomLeft"),
        (Alignment::BOTTOM_CENTER, "bottomCenter"),
        (Alignment::BOTTOM_RIGHT, "bottomRight"),
    ];
    doc("Alignment")
        .description(".alignment(..) slots non-positioned children into any of the nine anchor points — the stacking shorthand for badges, dots and floating chips.")
        .body(
            wrap(
                slots
                    .into_iter()
                    .map(|(alignment, label)| {
                        column(children![
                            stage(
                                92.0,
                                92.0,
                                stack(children![chip(palette::PURPLE, 26.0, 26.0)])
                                    .alignment(alignment),
                            )
                            .into_widget(),
                            gap_h(6.0),
                            muted(label.to_string()).size(10.5).into_widget(),
                        ])
                        .main_axis_size(MainAxisSize::Min)
                        .into_widget()
                    })
                    .collect::<Vec<_>>(),
            )
            .spacing(10.0)
            .run_spacing(12.0),
        )
}

fn positioned_section() -> impl IntoWidget {
    doc("Positioned")
        .description("Exact placement: edge pins (left/top, right/bottom), a full-size fill, a width-only span pinned left, and a vertical span pinned right — every combination the Flutter Positioned supports.")
        .body(
            stage(
                560.0,
                220.0,
                stack(children![
                    Positioned::fill(chip(palette::INDIGO, 0.0, 0.0)),
                    positioned(chip(palette::BLUE, 90.0, 60.0)).left(8.0).top(8.0),
                    positioned(chip(palette::GREEN, 90.0, 60.0)).right(8.0).bottom(8.0),
                    positioned(chip(palette::AMBER, 120.0, 40.0)).left(200.0).top(60.0),
                    positioned(chip(palette::PURPLE, 80.0, 60.0)).right(140.0).top(20.0),
                    positioned(center(text("center").color(palette::WHITE).semibold()))
                        .left(180.0)
                        .top(8.0)
                        .width(120.0)
                        .height(44.0),
                ]),
            ),
        )
        .into_widget()
}

fn _label(container: Container, text: &str) -> impl IntoWidget {
    column(children![
        container.into_widget(),
        gap_h(6.0),
        muted(text.to_string()).size(11.0).into_widget(),
    ])
    .main_axis_size(MainAxisSize::Min)
}

fn fit_and_zorder() -> impl IntoWidget {
    doc("Fit & z-order")
        .description("StackFit::Loose lets children keep their own size; .fit(StackFit::Expand) forces them to the stack's bounds. Later children paint on top — so the last chip wins overlaps.")
        .body(
            row(children![
                _label(
                    stage(
                        150.0,
                        110.0,
                        stack(children![
                            chip(palette::BLUE, 70.0, 40.0),
                            chip(palette::GREEN, 70.0, 40.0),
                        ]),
                    ),
                    "loose — children keep their size",
                ),
                gap_w(12.0),
                _label(
                    stage(
                        150.0,
                        110.0,
                        stack(children![
                            chip(palette::BLUE, 0.0, 0.0),
                            chip(palette::GREEN, 0.0, 0.0),
                        ])
                        .fit(StackFit::Expand),
                    ),
                    "expand — children fill the stack",
                ),
                gap_w(12.0),
                _label(
                    stage(
                        150.0,
                        110.0,
                        stack(children![
                            positioned(chip(palette::BLUE, 80.0, 40.0)).left(8.0).top(10.0),
                            positioned(chip(palette::GREEN, 80.0, 40.0)).left(26.0).top(26.0),
                            positioned(chip(palette::AMBER, 80.0, 40.0)).left(44.0).top(42.0),
                        ]),
                    ),
                    "later children paint on top",
                ),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
        .into_widget()
}

fn real_world() -> impl IntoWidget {
    let th = theme();
    doc("Real-world overlays")
        .description("The patterns Stack exists for: an online dot on an avatar, a gradient hero with caption and badge, and a corner ribbon on a card.")
        .body(
            row(children![
                column(children![
                    stack(children![
                        container()
                            .width(72.0)
                            .height(72.0)
                            .decoration(
                                BoxDecoration::new()
                                    .color(th.colors.primary)
                                    .shape(BoxShape::Circle),
                            )
                            .child(center(text("RK").color(palette::WHITE).semibold().size(20.0))),
                        positioned(
                            container()
                                .width(16.0)
                                .height(16.0)
                                .decoration(
                                    BoxDecoration::new()
                                        .color(palette::GREEN)
                                        .shape(BoxShape::Circle)
                                        .border(Border::new(th.colors.card, 2.0)),
                                ),
                        )
                        .right(2.0)
                        .bottom(2.0),
                    ])
                    .into_widget(),
                    gap_h(6.0),
                    muted("avatar + status dot").size(11.0).into_widget(),
                ])
                .main_axis_size(MainAxisSize::Min),
                gap_w(18.0),
                column(children![
                    container()
                        .width(220.0)
                        .height(110.0)
                        .decoration(
                            BoxDecoration::new()
                                .gradient(Gradient::linear(
                                    Alignment::TOP_LEFT,
                                    Alignment::BOTTOM_RIGHT,
                                    [th.colors.primary, th.colors.destructive],
                                ))
                                .radius(BorderRadius::all(th.radius)),
                        )
                        .child(
                            stack(children![
                                positioned(text("Dunes & Light").color(palette::WHITE).size(15.0).semibold())
                                    .left(12.0)
                                    .bottom(30.0),
                                positioned(text("Issue 04 · Field notes").color(palette::WHITE).size(11.0))
                                    .left(12.0)
                                    .bottom(12.0),
                                positioned(badge("new").variant(BadgeVariant::Secondary))
                                    .right(12.0)
                                    .top(12.0),
                            ]),
                        )
                        .into_widget(),
                    gap_h(6.0),
                    muted("hero banner + caption + badge").size(11.0).into_widget(),
                ])
                .main_axis_size(MainAxisSize::Min),
                gap_w(18.0),
                column(children![
                    container()
                        .width(180.0)
                        .height(110.0)
                        .decoration(
                            BoxDecoration::new()
                                .color(th.colors.card)
                                .border(Border::new(th.colors.border, 1.0))
                                .radius(BorderRadius::all(th.radius)),
                        )
                        .child(
                            stack(children![
                                center(column(children![
                                    text("Ribbon card").semibold().size(14.0),
                                    muted("corner tag").size(11.5),
                                ])
                                .main_axis_size(MainAxisSize::Min)),
                                positioned(
                                    container()
                                        .width(64.0)
                                        .height(22.0)
                                        .decoration(
                                            BoxDecoration::new()
                                                .color(th.colors.primary)
                                                .radius(BorderRadius { top_left: 0.0, top_right: 0.0, bottom_right: 0.0, bottom_left: th.radius }),
                                        )
                                        .child(center(text("PRO").color(palette::WHITE).size(10.5).bold())),
                                )
                                .right(0.0)
                                .top(0.0),
                            ]),
                        )
                        .into_widget(),
                    gap_h(6.0),
                    muted("card + corner ribbon").size(11.0).into_widget(),
                ])
                .main_axis_size(MainAxisSize::Min),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}
