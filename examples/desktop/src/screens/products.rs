//! The **Products** screen — the flagship data table: live search, category and
//! stock-status filters, sortable columns, status badges, pagination, and row-click
//! to a right-hand detail sheet. All filtering/sorting/paging is done over the store's
//! product signal, so any edit (stock, price, delete) re-renders the table instantly.

use pebbles::prelude::*;

use crate::components;
use crate::model::{Product, StockStatus};
use crate::sheets::open_product_detail;
use crate::store;

pub fn products() -> impl IntoWidget {
    component(products_view)
}

fn products_view() -> impl IntoWidget {
    let c = theme().colors;
    let search = create_signal(String::new());
    let cat = create_signal(0_usize);
    let stock = create_signal(0_usize);
    let sort_col = create_signal(0_usize);
    let sort_dir = create_signal(SortDir::Asc);
    let page = create_signal(0_usize);
    let per_page = create_signal(10_usize); // rows per page (user-adjustable)

    let all = store::products();

    // Category options: "All" + the distinct categories present.
    let mut categories: Vec<String> = vec!["All categories".to_string()];
    for p in &all {
        if !categories.contains(&p.category) {
            categories.push(p.category.clone());
        }
    }

    // --- filter -------------------------------------------------------------
    let q = search.get().to_lowercase();
    let cat_sel = cat.get();
    let stock_sel = stock.get();
    let mut rows: Vec<Product> = all
        .into_iter()
        .filter(|p| {
            let matches_q = q.is_empty()
                || p.name.to_lowercase().contains(&q)
                || p.sku.to_lowercase().contains(&q)
                || p.brand.to_lowercase().contains(&q);
            let matches_cat = cat_sel == 0 || categories.get(cat_sel).is_some_and(|c| &p.category == c);
            matches_q && matches_cat && stock_matches(stock_sel, p.status())
        })
        .collect();

    // --- sort ---------------------------------------------------------------
    let dir = sort_dir.get();
    let col = sort_col.get();
    rows.sort_by(|a, b| {
        let o = match col {
            3 => a.price_cents.cmp(&b.price_cents),
            4 => a.stock.cmp(&b.stock),
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        };
        if dir == SortDir::Desc { o.reverse() } else { o }
    });

    // --- paginate -----------------------------------------------------------
    let total = rows.len();
    let size = per_page.get();
    let total_pages = total.div_ceil(size).max(1);
    let cur = page.get().min(total_pages - 1);
    let slice: Vec<Product> = rows.into_iter().skip(cur * size).take(size).collect();

    // --- table --------------------------------------------------------------
    let headers = ["Product", "SKU", "Category", "Price", "Stock", "Status", ""]
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let mut t = table(headers)
        .striped(true)
        .row_hover(true)
        .sortable(0)
        .sortable(3)
        .sortable(4)
        .sort(col, dir)
        .on_sort(move |col, dir| {
            sort_col.set(col);
            sort_dir.set(dir);
            page.set(0);
        })
        .empty(components::table_empty("No products match your filters."));

    for p in &slice {
        let id = p.id;
        t = t.row(vec![
            cell(name_cell(p)),
            Cell::from(p.sku.clone()),
            Cell::from(p.category.clone()),
            Cell::from(components::price(p.price_cents)),
            Cell::from(p.stock.to_string()),
            cell(components::stock_badge(p)),
            cell(
                button("View")
                    .variant(ButtonVariant::Outline)
                    .leading(lucide::EYE)
                    .on_pressed(move || open_product_detail(id)),
            ),
        ]);
    }

    // --- toolbar ------------------------------------------------------------
    let toolbar = row(children![
        components::search_field(search, page, "Search name, SKU or brand…", 280.0),
        gap_w(10.0),
        select(categories.clone()).value(cat_sel).width(170.0).on_changed(move |i, _| {
            cat.set(i);
            page.set(0);
        }),
        gap_w(10.0),
        select(vec!["All stock", "In stock", "Low stock", "Out of stock"])
            .value(stock_sel)
            .width(150.0)
            .on_changed(move |i, _| {
                stock.set(i);
                page.set(0);
            }),
        spacer(),
        text(format!("{total} products")).size(13.0).color(c.muted_foreground),
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

/// The first column: thumbnail + name (tap to open) + brand. No width cap — the
/// column sizes to its content (the table scrolls horizontally if the row is wide).
fn name_cell(p: &Product) -> AnyWidget {
    let c = theme().colors;
    let id = p.id;
    pressable(
        row(children![
            components::thumb(p, 34.0),
            gap_w(10.0),
            column(children![
                text(p.name.clone()).size(13.5).weight(600.0).color(c.foreground),
                text(p.brand.clone()).size(12.0).color(c.muted_foreground),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center),
    )
    .radius(8.0)
    .on_tap(move || open_product_detail(id))
    .into_widget()
}

fn stock_matches(sel: usize, status: StockStatus) -> bool {
    match sel {
        1 => status == StockStatus::InStock,
        2 => status == StockStatus::LowStock,
        3 => status == StockStatus::OutOfStock,
        _ => true,
    }
}
