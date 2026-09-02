use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

fn chip(color: Color, w: f64, h: f64) -> Container {
    Container::new()
        .width(w)
        .height(h)
        .decoration(BoxDecoration::new().color(color).radius(BorderRadius::all(6.0)))
}

fn stage(w: f64, h: f64, child: impl IntoWidget) -> Container {
    Container::new()
        .width(w)
        .height(h)
        .padding(EdgeInsets::all(6.0))
        .decoration(
            BoxDecoration::new()
                .border(Border::new(theme().colors.border, 1.0))
                .radius(BorderRadius::all(theme().radius)),
        )
        .clip()
        .child(child)
}

/// Like [`stage`], but no inner padding — for widgets whose whole point is to
/// reach the box's edges (FittedBox's `Fill`/`Cover` must touch the border).
fn stage_tight(w: f64, h: f64, child: impl IntoWidget) -> Container {
    Container::new()
        .width(w)
        .height(h)
        .decoration(
            BoxDecoration::new()
                .border(Border::new(theme().colors.border, 1.0))
                .radius(BorderRadius::all(theme().radius)),
        )
        .clip()
        .child(child)
}

pub fn boxes() -> Element {
    screen("Boxes & Sizing")
        .description(
            "The single-child box protocol: AspectRatio, SizedBox, Padding, Align/Center, ConstrainedBox and Transform — the building blocks every layout above composes from. Plus the fit/intrinsic tier: FittedBox, FractionallySizedBox, IntrinsicWidth/Height, LimitedBox and OverflowBox.",
        )
        .body(children![
            aspect(),
            sized_box(),
            padding(),
            align(),
            constrained(),
            transforms(),
            fitted(),
            fractional(),
            intrinsic(),
            overflow_boxes(),
        ])
}

fn aspect() -> impl IntoWidget {
    doc("AspectRatio")
        .description(".aspect_ratio(r) forces width:height = r, so media frames stay true at any width — 16:9 video, 4:3 photos, 1:1 avatars, 21:9 cinema.")
        .body(
            row({
                let mut items: Vec<AnyWidget> = Vec::new();
                for (r, label, color) in [
                    (16.0 / 9.0, "16:9", palette::BLUE),
                    (4.0 / 3.0, "4:3", palette::GREEN),
                    (1.0, "1:1", palette::AMBER),
                    (21.0 / 9.0, "21:9", palette::PURPLE),
                ] {
                    items.push(
                        column(children![
                            Container::new()
                                .width(150.0)
                                .child(
                                    aspect_ratio(
                                        r,
                                        center(text(label.to_string()).color(palette::WHITE).size(13.0).semibold())
                                            .styled(style().background(color).radius_all(8.0)),
                                    ),
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
            .spacing(14.0),
        )
}

fn sized_box() -> impl IntoWidget {
    doc("SizedBox")
        .description("The explicit box: .exact() pins both dimensions, .square() one dimension, .expand() fills the parent, .shrink() collapses to nothing.")
        .body(
            row(children![
                stage(140.0, 90.0, SizedBox::exact(90.0, 40.0, chip(palette::BLUE, 0.0, 0.0))),
                gap_w(12.0),
                stage(140.0, 90.0, SizedBox::square(56.0, chip(palette::GREEN, 0.0, 0.0))),
                gap_w(12.0),
                stage(140.0, 90.0, SizedBox::expand(chip(palette::AMBER, 0.0, 0.0))),
                gap_w(12.0),
                stage(140.0, 90.0, SizedBox::shrink()),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn padding() -> impl IntoWidget {
    let th = theme();
    let inner = chip(palette::TEAL, 0.0, 0.0);
    doc("Padding")
        .description("EdgeInsets around a child — .all(), .symmetric() and .only() cover every inset pattern; the bordered boxes show exactly where the space goes.")
        .body(
            row(children![
                Container::new()
                    .padding(EdgeInsets::all(14.0))
                    .decoration(BoxDecoration::new().border(Border::new(th.colors.border, 1.0)))
                    .child(inner.clone().width(60.0).height(40.0))
                    .into_widget(),
                gap_w(12.0),
                Container::new()
                    .padding(EdgeInsets::symmetric(26.0, 8.0))
                    .decoration(BoxDecoration::new().border(Border::new(th.colors.border, 1.0)))
                    .child(inner.clone().width(60.0).height(40.0))
                    .into_widget(),
                gap_w(12.0),
                Container::new()
                    .padding(EdgeInsets::only(6.0, 4.0, 24.0, 20.0))
                    .decoration(BoxDecoration::new().border(Border::new(th.colors.border, 1.0)))
                    .child(inner.clone().width(60.0).height(40.0))
                    .into_widget(),
                gap_w(12.0),
                muted("all(14) · symmetric(26,8) · only(6,4,24,20)").size(11.5).into_widget(),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn align() -> impl IntoWidget {
    doc("Align & Center")
        .description("Align places a single child at one of the nine anchors; center() is the most common one. Unlike Stack, there's no overlap — this is about positioning within leftover space.")
        .body(
            wrap(
                [Alignment::TOP_LEFT, Alignment::TOP_CENTER, Alignment::TOP_RIGHT, Alignment::CENTER_LEFT, Alignment::CENTER, Alignment::CENTER_RIGHT, Alignment::BOTTOM_LEFT, Alignment::BOTTOM_CENTER, Alignment::BOTTOM_RIGHT]
                    .into_iter()
                    .map(|alignment| {
                        stage(
                            84.0,
                            84.0,
                            Align::new(alignment, chip(palette::PINK, 24.0, 24.0)),
                        )
                        .into_widget()
                    })
                    .collect::<Vec<_>>(),
            )
            .spacing(10.0)
            .run_spacing(10.0),
        )
}

fn constrained() -> impl IntoWidget {
    doc("ConstrainedBox")
        .description("Imposes additional min/max constraints on a child. Clamp a paragraph's width to wrap it early; force a minimum so a widget can't shrink below its touch target.")
        .body(
            row(children![
                stage(
                    220.0,
                    120.0,
                    ConstrainedBox::new(
                        BoxConstraints { min_width: 0.0, max_width: 140.0, min_height: 0.0, max_height: f64::INFINITY },
                        text("This paragraph is constrained to 140px wide, so it wraps early even though the stage is wider.".to_string())
                            .size(12.5)
                            .line_height(1.4),
                    ),
                )
                .into_widget(),
                gap_w(12.0),
                stage(
                    220.0,
                    120.0,
                    ConstrainedBox::new(
                        BoxConstraints { min_width: 180.0, max_width: f64::INFINITY, min_height: 0.0, max_height: f64::INFINITY },
                        Container::new()
                            .width(40.0)
                            .height(30.0)
                            .decoration(BoxDecoration::new().color(palette::INDIGO).radius(BorderRadius::all(6.0))),
                    ),
                )
                .into_widget(),
                gap_w(12.0),
                muted("max 140 → wraps early · min 180 → a 40px child is forced wider").size(11.5).into_widget(),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
        .into_widget()
}

fn transforms() -> impl IntoWidget {
    doc("Transform")
        .description("Affine transforms paint anywhere: rotate, scale, translate — combined for a tilted, zoomed, offset chip. Painting is transformed; layout is not.")
        .body(
            row(children![
                transform(Affine::rotate(0.21), chip(palette::BLUE, 90.0, 60.0)),
                gap_w(34.0),
                transform(Affine::scale(1.3), chip(palette::GREEN, 90.0, 60.0)),
                gap_w(34.0),
                transform(Affine::translate((28.0, 14.0)), chip(palette::AMBER, 90.0, 60.0)),
                gap_w(34.0),
                transform(
                    Affine::rotate(0.26) * Affine::translate((16.0, -6.0)) * Affine::scale(0.9),
                    chip(palette::PURPLE, 90.0, 60.0),
                ),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn fitted() -> impl IntoWidget {
    let banner = center(text("Pebbles".to_string()).color(palette::WHITE).size(20.0).bold());
    doc("FittedBox")
        .description("Lays its child out at its natural size, then scales it to fit the box it is given — Contain, Cover, Fill, None, FitWidth, FitHeight, ScaleDown. The classic responsive fix: one banner, any parent.")
        .body(
            wrap(
                [
                    (BoxFit::Contain, "Contain"),
                    (BoxFit::Cover, "Cover"),
                    (BoxFit::Fill, "Fill"),
                    (BoxFit::None, "None"),
                    (BoxFit::FitWidth, "FitWidth"),
                    (BoxFit::FitHeight, "FitHeight"),
                    (BoxFit::ScaleDown, "ScaleDown"),
                ]
                .into_iter()
                .map(|(fit, label)| {
                    column(children![
                        stage_tight(
                            150.0,
                            90.0,
                            fitted_box(
                                Container::new()
                                    .width(170.0)
                                    .height(64.0)
                                    .decoration(BoxDecoration::new()
                                        .color(palette::BLUE)
                                        .radius(BorderRadius::all(10.0)))
                                    .child(banner.clone()),
                            )
                            .fit(fit),
                        ),
                        gap_h(6.0),
                        muted(label).size(11.0).into_widget(),
                    ])
                    .main_axis_size(MainAxisSize::Min)
                    .into_widget()
                })
                .collect::<Vec<_>>(),
            )
            .spacing(12.0)
            .run_spacing(12.0),
        )
}

fn fractional() -> impl IntoWidget {
    doc("FractionallySizedBox")
        .description("Sizes a child to a fraction of the incoming constraints: .width_factor(0.5) is half the available width; unset axes pass through. Alignment places it within the leftover space.")
        .body(
            row(children![
                stage(
                    160.0,
                    100.0,
                    fractionally_sized_box(chip(palette::BLUE, 0.0, 0.0))
                        .width_factor(0.5)
                        .height_factor(1.0),
                )
                .into_widget(),
                gap_w(12.0),
                stage(
                    160.0,
                    100.0,
                    fractionally_sized_box(chip(palette::GREEN, 0.0, 0.0))
                        .width_factor(0.4)
                        .height_factor(0.4),
                )
                .into_widget(),
                gap_w(12.0),
                stage(
                    160.0,
                    100.0,
                    fractionally_sized_box(chip(palette::AMBER, 0.0, 0.0))
                        .width_factor(0.3)
                        .height_factor(0.3)
                        .alignment(Alignment::TOP_RIGHT),
                )
                .into_widget(),
                gap_w(12.0),
                muted("½×1 · ⅖×⅖ · 30% pinned top-right").size(11.5).into_widget(),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn intrinsic() -> impl IntoWidget {
    doc("IntrinsicWidth & IntrinsicHeight")
        .description("Sizes a child to the size it would choose if nobody constrained it: a column shrink-wraps to its widest line; a row stretches to its tallest sibling. The 'how big would you be?' pass.")
        .body(
            row(children![
                stage(
                    260.0,
                    140.0,
                    intrinsic_width(column(children![
                        text("A column as wide as".to_string()).size(12.5),
                        text("its widest line of text".to_string()).size(12.5),
                        text("— shrink-wrapped exactly.".to_string()).size(12.5),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min)),
                )
                .into_widget(),
                gap_w(12.0),
                stage(
                    260.0,
                    140.0,
                    intrinsic_height(row(children![
                        SizedBox::square(44.0, chip(palette::INDIGO, 0.0, 0.0)),
                        gap_w(10.0),
                        SizedBox::exact(
                            44.0,
                            84.0,
                            chip(palette::TEAL, 0.0, 0.0),
                        ),
                        gap_w(10.0),
                        SizedBox::square(44.0, chip(palette::PINK, 0.0, 0.0)),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min)),
                )
                .into_widget(),
                gap_w(12.0),
                muted("width → widest child · height → tallest child, siblings stretch to match").size(11.5).into_widget(),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn overflow_boxes() -> impl IntoWidget {
    doc("LimitedBox & OverflowBox")
        .description("LimitedBox caps a child only on unbounded axes (lists, scroll views) — bounded axes pass through. OverflowBox lets a child exceed its box without clipping: pop a badge out of an avatar corner.")
        .body(
            row(children![
                stage(
                    180.0,
                    110.0,
                    limited_box(chip(palette::BLUE, 0.0, 0.0).width(400.0).height(30.0))
                        .max_width(160.0),
                )
                .into_widget(),
                gap_w(12.0),
                Container::new()
                    .width(120.0)
                    .height(110.0)
                    .padding(EdgeInsets::all(10.0))
                    .decoration(BoxDecoration::new()
                        .border(Border::new(theme().colors.border, 1.0))
                        .radius(BorderRadius::all(theme().radius)))
                    .child(
                        overflow_box(
                            chip(palette::RED, 0.0, 0.0).width(150.0).height(46.0),
                        )
                        .alignment(Alignment::TOP_LEFT),
                    )
                    .into_widget(),
                gap_w(12.0),
                muted("LimitedBox: 400px child capped at 160 on the unbounded axis · OverflowBox: child overhangs left & top").size(11.5).into_widget(),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}
