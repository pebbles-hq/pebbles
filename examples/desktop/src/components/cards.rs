//! Card surfaces: the table card (rounds a raw table), the dashboard KPI card, and a
//! titled content panel.

use pebbles::prelude::*;

use super::mix;

/// The card radius and inner padding shared by the table cards.
const CARD_RADIUS: f64 = 14.0;
const CARD_PADDING: f64 = 6.0;

/// Wrap a **raw** table in the app's card: a bordered, rounded surface with padding,
/// plus an inner rounded clip whose radius is the card's minus the padding
/// (`14 - 6 = 8`) — so the table's header-top and last-row-bottom corners round
/// concentrically with the card. Styling lives here, on the developer side; the table
/// widget itself stays unstyled.
pub fn table_card(table: impl IntoWidget) -> impl IntoWidget {
    let c = theme().colors;
    container()
        .decoration(
            BoxDecoration::new()
                .color(c.card)
                .border(Border::new(c.border, 1.0))
                .radius(BorderRadius::all(CARD_RADIUS)),
        )
        .padding(EdgeInsets::all(CARD_PADDING))
        .child(
            container()
                .decoration(BoxDecoration::new().radius(BorderRadius::all(CARD_RADIUS - CARD_PADDING)))
                .clip()
                .child(table),
        )
}

/// A dashboard metric: label + big value + a colored icon chip, with a sub-line.
pub fn kpi_card(label: &str, value: &str, sub: &str, ic: IconData, color: Color) -> impl IntoWidget {
    let c = theme().colors;
    container()
        .decoration(
            BoxDecoration::new()
                .color(c.card)
                .border(Border::new(c.border, 1.0))
                .radius(BorderRadius::all(14.0)),
        )
        .padding(EdgeInsets::all(18.0))
        // No fixed width — the card fills its cell in the responsive grid.
        .child(
            column(children![
                row(children![
                    text(label.to_string()).size(13.0).color(c.muted_foreground),
                    spacer(),
                    container()
                        .decoration(
                            BoxDecoration::new()
                                .color(mix(c.card, color, 0.16))
                                .radius(BorderRadius::all(9.0)),
                        )
                        .padding(EdgeInsets::all(7.0))
                        .child(icon(ic).size(17.0).color(color)),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Center),
                gap_h(12.0),
                text(value.to_string()).size(25.0).weight(700.0).color(c.foreground),
                gap_h(4.0),
                text(sub.to_string()).size(12.5).color(c.muted_foreground),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        )
}

/// A titled content card: title + subtitle over a body slot.
pub fn panel(title: &str, subtitle: &str, body: AnyWidget) -> AnyWidget {
    let c = theme().colors;
    container()
        .decoration(
            BoxDecoration::new()
                .color(c.card)
                .border(Border::new(c.border, 1.0))
                .radius(BorderRadius::all(14.0)),
        )
        .padding(EdgeInsets::all(18.0))
        .child(
            column(children![
                text(title.to_string()).size(15.0).weight(700.0).color(c.foreground),
                gap_h(2.0),
                text(subtitle.to_string()).size(12.5).color(c.muted_foreground),
                gap_h(14.0),
                body,
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min),
        )
        .into_widget()
}
