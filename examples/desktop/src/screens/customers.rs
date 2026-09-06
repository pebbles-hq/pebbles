//! The **Customers** screen — a searchable, paginated table; each row opens the
//! customer's detail sheet (their profile, contact info and orders).

use pebbles::prelude::*;

use crate::components;
use crate::model::Customer;
use crate::sheets::open_customer_detail;
use crate::store;

pub fn customers() -> impl IntoWidget {
    component(customers_view)
}

fn customers_view() -> impl IntoWidget {
    let c = theme().colors;
    let search = create_signal(String::new());
    let page = create_signal(0_usize);
    let per_page = create_signal(10_usize); // rows per page (user-adjustable)

    let q = search.get().to_lowercase();
    let rows: Vec<Customer> = store::customers()
        .into_iter()
        .filter(|cu| {
            q.is_empty()
                || cu.name.to_lowercase().contains(&q)
                || cu.company.to_lowercase().contains(&q)
                || cu.email.to_lowercase().contains(&q)
        })
        .collect();

    let total = rows.len();
    let size = per_page.get();
    let total_pages = total.div_ceil(size).max(1);
    let cur = page.get().min(total_pages - 1);
    let slice: Vec<Customer> = rows.into_iter().skip(cur * size).take(size).collect();

    let headers =
        ["Customer", "Email", "Orders", "Spent", ""].iter().map(|s| s.to_string()).collect::<Vec<_>>();
    // Columns size to their content (full values, no truncation); the table scrolls
    // horizontally if they don't all fit.
    let mut t = table(headers)
        .striped(true)
        .row_hover(true)
        .empty(components::table_empty("No customers match your search."));
    for cu in &slice {
        let id = cu.id;
        t = t.row(vec![
            cell(name_cell(cu)),
            Cell::from(cu.email.clone()),
            Cell::from(store::customer_order_count(cu.id).to_string()),
            Cell::from(components::price(store::customer_spent_cents(cu.id))),
            cell(
                button("View")
                    .variant(ButtonVariant::Outline)
                    .leading(lucide::EYE)
                    .on_pressed(move || open_customer_detail(id)),
            ),
        ]);
    }

    let toolbar = row(children![
        components::search_field(search, page, "Search name, company or email…", 300.0),
        spacer(),
        text(format!("{total} customers")).size(13.0).color(c.muted_foreground),
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

fn name_cell(cu: &Customer) -> AnyWidget {
    let c = theme().colors;
    let id = cu.id;
    let initials: String = cu.name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect();
    pressable(
        row(children![
            container()
                .decoration(BoxDecoration::new().color(palette::violet::S500).shape(BoxShape::Circle))
                .width(32.0)
                .height(32.0)
                .alignment(Alignment::CENTER)
                .child(text(initials).size(12.5).weight(700.0).color(Color::WHITE)),
            gap_w(10.0),
            column(children![
                text(cu.name.clone()).size(13.5).weight(600.0).color(c.foreground),
                text(cu.company.clone()).size(12.0).color(c.muted_foreground),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center),
    )
    .radius(8.0)
    .on_tap(move || open_customer_detail(id))
    .into_widget()
}
