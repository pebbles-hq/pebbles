//! Small shared UI atoms reused across the screens: money labels, status pills, KPI
//! cards, star ratings and product thumbnails.

use pebbles::prelude::*;

use crate::model::{self, OrderStatus, Product, StockStatus};
use crate::store;

/// A money label in the current currency, e.g. `$1,234.56`.
pub fn price(cents: i64) -> String {
    model::money_sym(cents, &store::symbol())
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
        .width(236.0)
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
