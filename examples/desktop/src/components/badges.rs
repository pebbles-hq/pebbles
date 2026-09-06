//! Status pills for stock health and order status.

use pebbles::prelude::*;

use crate::model::{OrderStatus, Product, StockStatus};

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
