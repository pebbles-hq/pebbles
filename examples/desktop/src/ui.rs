//! Small shared UI atoms reused across the screens: money labels, status pills, KPI
//! cards, star ratings and product thumbnails.

use pebbles::prelude::*;

use crate::model::{self, OrderStatus, Product, StockStatus};
use crate::store;

/// A money label in the current currency, e.g. `$1,234.56`.
pub fn price(cents: i64) -> String {
    model::money_sym(cents, &store::symbol())
}

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

// ---------------------------------------------------------------------------
// Status pills
// ---------------------------------------------------------------------------

/// A solid rounded status pill (white text on a status color).
pub fn status_pill(label: &str, color: Color) -> impl IntoWidget {
    container()
        .decoration(BoxDecoration::new().color(color).radius(BorderRadius::all(999.0)))
        .padding(EdgeInsets::symmetric(10.0, 3.0))
        .child(text(label.to_string()).size(11.5).weight(600.0).color(Color::WHITE))
}

pub fn stock_color(status: StockStatus) -> Color {
    match status {
        StockStatus::InStock => palette::emerald::S500,
        StockStatus::LowStock => palette::amber::S500,
        StockStatus::OutOfStock => palette::rose::S500,
    }
}

pub fn stock_badge(p: &Product) -> AnyWidget {
    let s = p.status();
    status_pill(s.label(), stock_color(s)).into_widget()
}

pub fn order_color(status: OrderStatus) -> Color {
    match status {
        OrderStatus::Pending => palette::slate::S400,
        OrderStatus::Paid => palette::sky::S500,
        OrderStatus::Shipped => palette::violet::S500,
        OrderStatus::Delivered => palette::emerald::S500,
        OrderStatus::Cancelled => palette::rose::S500,
    }
}

pub fn order_badge(status: OrderStatus) -> AnyWidget {
    status_pill(status.label(), order_color(status)).into_widget()
}

// ---------------------------------------------------------------------------
// KPI card
// ---------------------------------------------------------------------------

/// A **responsive grid**: lay `cards` out in `desktop` / `tablet` / `mobile` columns
/// depending on the window's [`Breakpoint`], each card stretching to fill its column
/// and wrapping to new rows. Reads `breakpoint()` (reactive on the window size), so it
/// re-flows the moment the window crosses a breakpoint — no polling, no frame lag.
pub fn responsive_grid(
    spacing: f64,
    mobile: usize,
    tablet: usize,
    desktop: usize,
    cards: Vec<AnyWidget>,
) -> impl IntoWidget {
    let n = cards.len().max(1);
    let cols = breakpoint().select(mobile, tablet, desktop).clamp(1, n);

    let mut rows: Vec<AnyWidget> = Vec::new();
    let mut it = cards.into_iter();
    let mut remaining = n;
    let mut first = true;
    while remaining > 0 {
        if !first {
            rows.push(gap_h(spacing).into_widget());
        }
        first = false;
        let take = cols.min(remaining);
        let mut cells: Vec<AnyWidget> = Vec::new();
        for col in 0..cols {
            if col > 0 {
                cells.push(gap_w(spacing).into_widget());
            }
            if col < take {
                // Each card fills an equal share of the row width.
                cells.push(Expanded::new(it.next().unwrap()).into_widget());
            } else {
                // Empty slots on the last row keep the card widths consistent.
                cells.push(Expanded::new(gap_h(0.0)).into_widget());
            }
        }
        remaining -= take;
        rows.push(row(cells).cross_axis_alignment(CrossAxisAlignment::Stretch).into_widget());
    }
    column(rows).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min)
}

/// A dashboard metric: label + big value + a colored icon chip, with an optional
/// sub-line.
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

/// Blend `t` of `over` onto `base` (a light tint for icon chips / washes).
pub fn mix(base: Color, over: Color, t: f64) -> Color {
    let t = t as f32;
    let [br, bg, bb, _] = base.components;
    let [or, og, ob, _] = over.components;
    Color::new([br + (or - br) * t, bg + (og - bg) * t, bb + (ob - bb) * t, 1.0])
}

// ---------------------------------------------------------------------------
// Rating + thumbnail
// ---------------------------------------------------------------------------

pub fn stars(rating: f64) -> impl IntoWidget {
    let c = theme().colors;
    row(children![
        text("★".to_string()).size(13.0).color(palette::amber::S500),
        gap_w(3.0),
        text(format!("{rating:.1}")).size(12.5).color(c.muted_foreground),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .main_axis_size(MainAxisSize::Min)
}

/// A rounded product thumbnail — its first image, with a colored placeholder tile
/// (also the offline fallback).
pub fn thumb(p: &Product, size: f64) -> AnyWidget {
    let c = theme().colors;
    let placeholder = container()
        .decoration(BoxDecoration::new().color(c.secondary).radius(BorderRadius::all(10.0)))
        .width(size)
        .height(size)
        .alignment(Alignment::CENTER)
        .child(icon(lucide::PACKAGE).size(size * 0.42).color(c.muted_foreground));

    match p.thumb() {
        Some(url) => container()
            .decoration(BoxDecoration::new().color(c.secondary).radius(BorderRadius::all(10.0)))
            .clip()
            .width(size)
            .height(size)
            .child(ImageView::network(url).fit(ImageFit::Cover).placeholder(placeholder))
            .into_widget(),
        None => placeholder.into_widget(),
    }
}
