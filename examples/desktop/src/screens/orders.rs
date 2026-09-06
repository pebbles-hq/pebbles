//! The **Orders** screen — a table of orders with search, a status filter, status
//! badges and pagination; each row opens the order's detail sheet.

use pebbles::prelude::*;

use crate::components;
use crate::model::{Order, OrderStatus};
use crate::sheets::open_order_detail;
use crate::store;

pub fn orders() -> impl IntoWidget {
    component(orders_view)
}

fn orders_view() -> impl IntoWidget {
    let c = theme().colors;
    let search = create_signal(String::new());
    let status = create_signal(0_usize); // 0 = All
    let page = create_signal(0_usize);
    let per_page = create_signal(10_usize); // rows per page (user-adjustable)

    let q = search.get().to_lowercase();
    let status_sel = status.get();

    let mut rows: Vec<Order> = store::orders()
        .into_iter()
        .filter(|o| {
            let name = store::customer(o.customer_id).map(|cu| cu.name).unwrap_or_default();
            let matches_q =
                q.is_empty() || o.code.to_lowercase().contains(&q) || name.to_lowercase().contains(&q);
            let matches_status =
                status_sel == 0 || OrderStatus::all().get(status_sel - 1).is_some_and(|s| *s == o.status);
            matches_q && matches_status
        })
        .collect();
    rows.sort_by_key(|o| std::cmp::Reverse(o.id)); // newest first

    let total = rows.len();
    let size = per_page.get();
    let total_pages = total.div_ceil(size).max(1);
    let cur = page.get().min(total_pages - 1);
    let slice: Vec<Order> = rows.into_iter().skip(cur * size).take(size).collect();

    let headers = ["Order", "Customer", "Date", "Items", "Total", "Status", ""]
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    // The customer name can be long → one line with an ellipsis.
    // Columns size to their content (full values, no truncation); the table scrolls
    // horizontally if they don't all fit.
    let mut t = table(headers)
        .striped(true)
        .row_hover(true)
        .empty(components::table_empty("No orders match your filters."));
    for o in &slice {
        let id = o.id;
        let customer = store::customer(o.customer_id).map(|cu| cu.name).unwrap_or_else(|| "—".into());
        t = t.row(vec![
            cell(link(&o.code, move || open_order_detail(id))),
            Cell::from(customer),
            Cell::from(o.date.clone()),
            Cell::from(o.item_count().to_string()),
            Cell::from(components::price(o.subtotal_cents())),
            cell(components::order_badge(o.status)),
            cell(
                button("View")
                    .variant(ButtonVariant::Outline)
                    .leading(lucide::EYE)
                    .on_pressed(move || open_order_detail(id)),
            ),
        ]);
    }

    // Status filter options: All + each status label.
    let mut status_opts = vec!["All statuses".to_string()];
    status_opts.extend(OrderStatus::all().iter().map(|s| s.label().to_string()));

    let toolbar = row(children![
        components::search_field(search, page, "Search order # or customer…", 280.0),
        gap_w(10.0),
        select(status_opts).value(status_sel).width(170.0).on_changed(move |i, _| {
            status.set(i);
            page.set(0);
        }),
        spacer(),
        text(format!("{total} orders")).size(13.0).color(c.muted_foreground),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center);

    let pager = components::table_pager(page, per_page, cur, total_pages, total);

    let card = components::table_card(t);

    scroll_view(
        container().padding(EdgeInsets::all(24.0)).child(
            column(children![toolbar, gap_h(16.0), card, pager])
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_size(MainAxisSize::Min),
        ),
    )
    .drag_scroll(true)
}

/// A primary-colored, clickable text cell (the order code).
fn link(label: &str, on_tap: impl Fn() + 'static) -> AnyWidget {
    pressable(text(label.to_string()).size(13.5).weight(600.0).color(theme().colors.primary))
        .radius(6.0)
        .on_tap(on_tap)
        .into_widget()
}
