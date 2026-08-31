use pebbles::prelude::*;

use crate::styles;
use crate::ui::{gap_h, gap_w, screen, section};

pub fn styling() -> Element {
    screen(
        "Styling",
        "One general Style value, applied to any widget — React-Native / CSS-like.",
        children![
            section(
                "SAME STYLE, ANY WIDGET — .styled(style) applies box properties",
                column(
                    children![
                        // A pill style on a Text...
                        text("styled Text").styled(styles::pill()),
                        gap_h(10.0),
                        // ...the same pill on a Row of icons...
                        row(children![icon(IconKind::Star).size(18.0), icon(IconKind::Check).size(18.0)]).min().spacing(8.0)
                            .styled(styles::pill()),
                        gap_h(10.0),
                        // ...and a card style on a Column.
                        column(children![title("Card via style()"), muted("background + border + radius + shadow")])
                            .start()
                            .min()
                            .styled(styles::card()),
                    ]).start().min().spacing(0.0),
            ),
            section(
                "GRADIENT · CIRCLE · STACKED SHADOWS — all via Style, on any widget",
                row(children![
                    center(text("Gradient").color(palette::WHITE).semibold())
                        .styled(style().size(150.0, 84.0).radius_all(14.0).gradient(Gradient::linear(
                            Alignment::TOP_LEFT,
                            Alignment::BOTTOM_RIGHT,
                            [theme().colors.primary, theme().colors.destructive],
                        ))),
                    center(icon(IconKind::Star).size(28.0).color(palette::WHITE))
                        .styled(style().size(84.0, 84.0).circle().background(theme().colors.primary)),
                    center(muted("two shadows")).styled(
                        style()
                            .size(150.0, 84.0)
                            .radius_all(14.0)
                            .background(theme().colors.card)
                            .shadow(BoxShadow::new(Color::from_rgba8(0, 0, 0, 45), Offset::new(0.0, 2.0), 8.0, 0.0))
                            .shadow(BoxShadow::new(theme().colors.primary, Offset::new(0.0, 12.0), 24.0, -8.0)),
                    ),
                ])
                .min()
                .spacing(16.0),
            ),
            section(
                "PER-SIDE BORDER · TRANSFORM (rotate) · BLEND — Style + Container",
                row(children![
                    // Per-side border: thick top+bottom, thin sides.
                    center(muted("per-side")).styled(
                        style().size(150.0, 84.0).radius_all(6.0).background(theme().colors.card).border(
                            Border::symmetric(
                                BorderSide::new(theme().colors.primary, 3.0),
                                BorderSide::new(theme().colors.border, 1.0),
                            ),
                        ),
                    ),
                    // A rotated box — still hit-testable — via Container.transform.
                    Container::new()
                        .width(84.0)
                        .height(84.0)
                        .alignment(Alignment::CENTER)
                        .decoration(
                            BoxDecoration::new()
                                .color(theme().colors.primary)
                                .radius(BorderRadius::all(12.0)),
                        )
                        .transform(Affine::rotate(0.15))
                        .child(icon(IconKind::Star).size(26.0).color(palette::WHITE)),
                    // Blend the gradient with what's behind it.
                    center(muted("multiply")).styled(
                        style()
                            .size(150.0, 84.0)
                            .radius_all(6.0)
                            .gradient(Gradient::horizontal([theme().colors.primary, theme().colors.destructive]))
                            .blend(BlendMode::Multiply),
                    ),
                ])
                .min()
                .spacing(16.0),
            ),
            section(
                "TEXT STYLE — text(..).style(style) applies text + box properties",
                text("Heading rendered from a global style").style(styles::heading()),
            ),
            section(
                "MERGE — layer styles, later wins (like stacking CSS classes)",
                text("card().merge(danger())")
                    .style(styles::card().merge(styles::danger()).padding_all(16.0)),
            ),
            section(
                "MUTED PANEL — reused anywhere",
                column(children![
                    text("This whole panel is one reusable style."),
                    gap_h(8.0),
                    muted("styles::muted_panel()"),
                ])
                .start()
                .min()
                .styled(styles::muted_panel()),
            ),
            section(
                "styles([a, b, c]) — compose layers left-to-right (RN style={[..]})",
                center(text("layered").color(palette::WHITE).semibold()).styled(styles([
                    style().size(200.0, 70.0).radius_all(10.0).background(theme().colors.muted_foreground),
                    style().background(theme().colors.primary), // wins
                    style().radius_all(20.0),                   // wins
                ])),
            ),
            section(
                "CONSTRAINTS · ASPECT RATIO · CURSOR",
                row(children![
                    center(muted("min 160×64")).styled(style().min_width(160.0).min_height(64.0).radius_all(8.0).background(theme().colors.card).border(Border::new(theme().colors.border, 1.0))),
                    center(muted("16:9")).styled(style().width(160.0).aspect_ratio(16.0 / 9.0).radius_all(8.0).background(theme().colors.secondary)),
                    center(muted("pointer cursor")).styled(style().size(150.0, 64.0).radius_all(8.0).background(theme().colors.secondary).cursor(Cursor::Pointer)),
                ])
                .min()
                .spacing(16.0),
            ),
            section(
                "TEXT PROPS — align · italic · underline · letter-spacing · max_lines",
                column(children![
                    text("Centered italic underlined").style(style().text_align(TextAlign::Center).italic(true).underline(true).font_size(15.0).width(360.0)),
                    gap_h(8.0),
                    text("W I D E   T R A C K I N G").style(style().letter_spacing(2.0).semibold()),
                    gap_h(8.0),
                    text("This paragraph is clamped to a single line via max_lines(1); the rest is dropped rather than wrapping onto a second line.")
                        .style(style().max_lines(1).width(360.0).color(theme().colors.muted_foreground)),
                ])
                .start()
                .min(),
            ),
            section(
                "COMPONENT .style — Card / Badge / Alert / TextField accept a Style",
                column(children![
                    card().child(text("card().style(red bg, no radius)")).style(style().background(palette::red::S50).radius_all(0.0).border(Border::new(palette::red::S300, 1.0))),
                    gap_h(10.0),
                    row(children![
                        badge("themed").variant(BadgeVariant::Secondary),
                        gap_w(8.0),
                        badge("styled").style(style().background(palette::emerald::S500)),
                    ]).min(),
                    gap_h(10.0),
                    text_field().placeholder("styled field").width(320.0).style(style().background(palette::amber::S50).border(Border::new(palette::amber::S400, 1.5))),
                ])
                .start()
                .min(),
            ),
        ],
    )
}
