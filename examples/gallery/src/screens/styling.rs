use pebbles::prelude::*;

use crate::styles;
use crate::ui::{gap_h, screen, section};

pub fn styling() -> impl IntoWidget {
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
                        row(children![icon(IconKind::Star).size(18.0), icon(IconKind::Check).size(18.0)]).main_axis_min().spacing(8.0)
                            .styled(styles::pill()),
                        gap_h(10.0),
                        // ...and a card style on a Column.
                        column(children![title("Card via style()"), muted("background + border + radius + shadow")])
                            .cross_axis_alignment(CrossAxisAlignment::Start)
                            .main_axis_min()
                            .styled(styles::card()),
                    ]).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min().spacing(0.0),
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
                .main_axis_min()
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
                .main_axis_min()
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
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_min()
                .styled(styles::muted_panel()),
            ),
        ],
    )
}
