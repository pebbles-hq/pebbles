//! The **Dashboard** — KPI cards computed live from the store, plus a low-stock
//! watch-list and a recent-orders feed. Everything is derived from the same signals
//! the tables use, so it stays in sync as data changes.

use pebbles::prelude::*;

use crate::sheets::{open_order_detail, open_product_detail};
use crate::store;
use crate::ui;

pub fn dashboard() -> impl IntoWidget {
    let revenue = ui::price(store::revenue_cents());
    let inv_value = ui::price(store::inventory_value_cents());
    let products = store::products().len();
    let low = store::low_stock().len();
    let pending = store::pending_orders();
    let customers = store::customers().len();

    let cards = vec![
        ui::kpi_card("Revenue", &revenue, "paid + fulfilled", lucide::TRENDING_UP, palette::emerald::S500)
            .into_widget(),
        ui::kpi_card("Inventory value", &inv_value, "at cost", lucide::BOXES, palette::sky::S500)
            .into_widget(),
        ui::kpi_card(
            "Products",
            &products.to_string(),
            &format!("{low} need attention"),
            lucide::PACKAGE,
            palette::violet::S500,
        )
        .into_widget(),
        ui::kpi_card(
            "Pending orders",
            &pending.to_string(),
            &format!("{customers} customers"),
            lucide::SHOPPING_CART,
            palette::amber::S500,
        )
        .into_widget(),
    ];

    let low_card = panel("Low stock", "Products at or below their reorder level", low_stock_list());
    let orders_card = panel("Recent orders", "The latest activity", recent_orders_list());

    scroll_view(
        container().padding(EdgeInsets::all(24.0)).child(
            column(children![
                wrap(cards).spacing(16.0).run_spacing(16.0),
                gap_h(20.0),
                row(children![Expanded::new(low_card), gap_w(20.0), Expanded::new(orders_card),])
                    .cross_axis_alignment(CrossAxisAlignment::Start),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min),
        ),
    )
    .drag_scroll(true)
}

/// A titled content card.
fn panel(title: &str, subtitle: &str, body: AnyWidget) -> AnyWidget {
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

fn low_stock_list() -> AnyWidget {
    let c = theme().colors;
    let items = store::low_stock();
    if items.is_empty() {
        return text("Everything is well stocked. 🎉").size(13.0).color(c.muted_foreground).into_widget();
    }
    let rows: Vec<AnyWidget> = items
        .iter()
        .take(6)
        .map(|p| {
            let id = p.id;
            pressable(
                container().padding(EdgeInsets::symmetric(0.0, 8.0)).child(
                    row(children![
                        ui::thumb(p, 30.0),
                        gap_w(10.0),
                        Expanded::new(
                            text(p.name.clone()).size(13.5).max_lines(1).ellipsis().color(c.foreground),
                        ),
                        gap_w(8.0),
                        text(format!("{} left", p.stock)).size(12.5).color(c.muted_foreground),
                        gap_w(10.0),
                        ui::stock_badge(p),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Center),
                ),
            )
            .radius(8.0)
            .on_tap(move || open_product_detail(id))
            .into_widget()
        })
        .collect();
    column(rows)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
}

fn recent_orders_list() -> AnyWidget {
    let c = theme().colors;
    let mut orders = store::orders();
    orders.sort_by_key(|o| std::cmp::Reverse(o.id));
    let rows: Vec<AnyWidget> = orders
        .iter()
        .take(6)
        .map(|o| {
            let id = o.id;
            let name = store::customer(o.customer_id).map(|cu| cu.name).unwrap_or_else(|| "—".into());
            let total = ui::price(o.subtotal_cents());
            let code = o.code.clone();
            let status = o.status;
            pressable(
                container().padding(EdgeInsets::symmetric(0.0, 8.0)).child(
                    row(children![
                        text(code).size(13.5).weight(600.0).color(c.foreground),
                        gap_w(10.0),
                        Expanded::new(
                            text(name).size(13.0).max_lines(1).ellipsis().color(c.muted_foreground)
                        ),
                        gap_w(8.0),
                        ui::order_badge(status),
                        gap_w(10.0),
                        text(total).size(13.0).weight(600.0).color(c.foreground),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Center),
                ),
            )
            .radius(8.0)
            .on_tap(move || open_order_detail(id))
            .into_widget()
        })
        .collect();
    column(rows)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
}
