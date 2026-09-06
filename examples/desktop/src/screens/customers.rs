//! The **Customers** screen — a searchable, paginated table; each row opens the
//! customer's detail sheet (their profile, contact info and orders).

use pebbles::prelude::*;

use crate::model::Customer;
use crate::sheets::open_customer_detail;
use crate::store;
use crate::ui;

const PER_PAGE: usize = 9;

pub fn customers() -> impl IntoWidget {
    component(customers_view)
}

fn customers_view() -> impl IntoWidget {
    let c = theme().colors;
    let search = create_signal(String::new());
    let page = create_signal(0_usize);

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
    let total_pages = total.div_ceil(PER_PAGE).max(1);
    let cur = page.get().min(total_pages - 1);
    let slice: Vec<Customer> = rows.into_iter().skip(cur * PER_PAGE).take(PER_PAGE).collect();

    let headers =
        ["Customer", "Email", "Orders", "Spent", ""].iter().map(|s| s.to_string()).collect::<Vec<_>>();
    // Email addresses can be long → keep them to one line with an ellipsis.
    let mut t =
        table(headers).striped(true).row_hover(true).overflow(1, CellOverflow::Ellipsis).empty(empty_state());
    for cu in &slice {
        let id = cu.id;
        t = t.row(vec![
            cell(name_cell(cu)),
            Cell::from(cu.email.clone()),
            Cell::from(store::customer_order_count(cu.id).to_string()),
            Cell::from(ui::price(store::customer_spent_cents(cu.id))),
            cell(button("View").variant(ButtonVariant::Ghost).on_pressed(move || open_customer_detail(id))),
        ]);
    }

    let toolbar = row(children![
        container().width(300.0).child(
            text_field()
                .leading(lucide::SEARCH)
                .placeholder("Search name, company or email…")
                .bind(search)
                .on_changed(move |_| page.set(0)),
        ),
        spacer(),
        text(format!("{total} customers")).size(13.0).color(c.muted_foreground),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center);

    let pager: AnyWidget = if total_pages > 1 {
        container()
            .padding(EdgeInsets::only(0.0, 14.0, 0.0, 0.0))
            .child(
                row(children![spacer(), pagination(cur + 1, total_pages).on_page(move |p| page.set(p - 1))])
                    .cross_axis_alignment(CrossAxisAlignment::Center),
            )
            .into_widget()
    } else {
        gap_h(0.0).into_widget()
    };

    let card = container()
        .decoration(
            BoxDecoration::new()
                .color(c.card)
                .border(Border::new(c.border, 1.0))
                .radius(BorderRadius::all(14.0)),
        )
        .padding(EdgeInsets::all(6.0))
        .child(t);

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
            Expanded::new(
                column(children![
                    text(cu.name.clone())
                        .size(13.5)
                        .weight(600.0)
                        .max_lines(1)
                        .ellipsis()
                        .soft_wrap(false)
                        .color(c.foreground),
                    text(cu.company.clone())
                        .size(12.0)
                        .max_lines(1)
                        .ellipsis()
                        .soft_wrap(false)
                        .color(c.muted_foreground),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min),
            ),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center),
    )
    .radius(8.0)
    .on_tap(move || open_customer_detail(id))
    .into_widget()
}

fn empty_state() -> AnyWidget {
    let c = theme().colors;
    container()
        .padding(EdgeInsets::all(30.0))
        .alignment(Alignment::CENTER)
        .child(text("No customers match your search.").size(13.5).color(c.muted_foreground))
        .into_widget()
}
