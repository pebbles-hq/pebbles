//! The right-hand **detail sheets** — comprehensive drill-downs opened from a table
//! row (or the global search). Each reads the store reactively, so edits made here
//! (stock, status) update the underlying table live.

use pebbles::prelude::*;

use crate::components;
use crate::model::{Order, OrderStatus};
use crate::store;

// ===========================================================================
// Product detail
// ===========================================================================

pub fn open_product_detail(id: i64) {
    sheet(component_props(product_view, id)).side(Side::Right).size(480.0).title("Product").open();
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn product_view(id: &i64) -> AnyWidget {
    let id = *id;
    let c = theme().colors;
    let selected = create_signal(0_usize); // gallery selection

    let Some(p) = store::product(id) else {
        return empty_sheet("This product no longer exists.");
    };

    // Gallery: a big selected image + a row of tappable thumbnails.
    let sel = selected.get().min(p.images.len().saturating_sub(1));
    let hero: AnyWidget = match p.images.get(sel) {
        Some(url) => container()
            .decoration(BoxDecoration::new().color(c.secondary).radius(BorderRadius::all(14.0)))
            .clip()
            .height(220.0)
            .child(
                ImageView::network(url.clone()).fit(ImageFit::Cover).placeholder(
                    container()
                        .color(c.secondary)
                        .alignment(Alignment::CENTER)
                        .child(icon(lucide::IMAGE).size(30.0).color(c.muted_foreground)),
                ),
            )
            .into_widget(),
        None => container()
            .decoration(BoxDecoration::new().color(c.secondary).radius(BorderRadius::all(14.0)))
            .height(220.0)
            .alignment(Alignment::CENTER)
            .child(icon(lucide::PACKAGE).size(40.0).color(c.muted_foreground))
            .into_widget(),
    };

    let thumbs: Vec<AnyWidget> = p
        .images
        .iter()
        .enumerate()
        .map(|(i, url)| {
            let active = i == sel;
            let border = if active { c.primary } else { c.border };
            pressable(
                container()
                    .decoration(
                        BoxDecoration::new()
                            .color(c.secondary)
                            .border(Border::new(border, if active { 2.0 } else { 1.0 }))
                            .radius(BorderRadius::all(9.0)),
                    )
                    .clip()
                    .width(56.0)
                    .height(56.0)
                    .child(ImageView::network(url.clone()).fit(ImageFit::Cover)),
            )
            .radius(9.0)
            .on_tap(move || selected.set(i))
            .into_widget()
        })
        .collect();

    // Header: name, brand · category, stock badge + rating.
    let header = column(children![
        row(children![
            text(p.name.clone()).size(19.0).weight(700.0).color(c.foreground),
            spacer(),
            components::stock_badge(&p),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start),
        gap_h(4.0),
        row(children![
            text(format!("{} · {}", p.brand, p.category)).size(13.0).color(c.muted_foreground),
            spacer(),
            components::stars(p.rating),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center),
        gap_h(2.0),
        text(format!("SKU {}", p.sku)).size(12.0).color(c.muted_foreground),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .main_axis_size(MainAxisSize::Min);

    // Price / cost / margin metrics.
    let metrics = row(children![
        metric("Price", &components::price(p.price_cents)),
        metric("Cost", &components::price(p.cost_cents)),
        metric("Margin", &components::price(p.margin_cents())),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Start);

    // Live stock stepper.
    let stepper = row(children![
        text(format!("Stock: {}", p.stock)).size(14.0).weight(600.0).color(c.foreground),
        spacer(),
        icon_button(lucide::MINUS)
            .variant(ButtonVariant::Outline)
            .on_pressed(move || store::adjust_stock(id, -1)),
        gap_w(8.0),
        icon_button(lucide::PLUS)
            .variant(ButtonVariant::Outline)
            .on_pressed(move || store::adjust_stock(id, 1)),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center);

    // Edit form (name / price / reorder level) — real, persisted on Save.
    let name = create_signal(p.name.clone());
    let price = create_signal(dollars(p.price_cents));
    let reorder = create_signal(p.reorder_level.to_string());
    let save = move || {
        let cents = parse_dollars(&price.peek());
        let ro = reorder.peek().trim().parse::<i64>().unwrap_or(0);
        store::update_product(id, &name.peek(), cents, store::product(id).map(|x| x.stock).unwrap_or(0), ro);
        toast("Product updated").show();
    };

    // Recent orders that included this product — the item's "history".
    let history = order_history(id);

    let mut body: Vec<AnyWidget> = vec![
        hero,
        gap_h(10.0).into_widget(),
        row(thumbs).main_axis_size(MainAxisSize::Min).into_widget(),
        gap_h(16.0).into_widget(),
        header.into_widget(),
        gap_h(16.0).into_widget(),
        metrics.into_widget(),
        gap_h(16.0).into_widget(),
        stepper.into_widget(),
        gap_h(16.0).into_widget(),
        divider(),
        section_label("Description"),
        text(p.description.clone()).size(13.5).line_height(1.5).color(c.foreground).into_widget(),
        gap_h(16.0).into_widget(),
        divider(),
        section_label("Edit"),
        field(text_field().bind(name)).label("Name").into_widget(),
        gap_h(10.0).into_widget(),
        row(children![
            Expanded::new(field(text_field().bind(price).kind(InputKind::Number)).label("Price")),
            gap_w(10.0),
            Expanded::new(field(text_field().bind(reorder).kind(InputKind::Number)).label("Reorder at")),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .into_widget(),
        gap_h(12.0).into_widget(),
        button("Save changes").on_pressed(save).into_widget(),
        gap_h(16.0).into_widget(),
        divider(),
        section_label("Order history"),
        history,
        gap_h(20.0).into_widget(),
        row(children![
            button("Reorder stock")
                .variant(ButtonVariant::Secondary)
                .leading(lucide::REFRESH_CW)
                .on_pressed(move || store::reorder(id)),
            spacer(),
            button("Delete").variant(ButtonVariant::Destructive).on_pressed(move || confirm_delete(id)),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .into_widget(),
        gap_h(12.0).into_widget(),
    ];
    body.insert(0, gap_h(0.0).into_widget());

    scroll_view(
        column(body).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min),
    )
    .drag_scroll(true)
    .into_widget()
}

fn confirm_delete(id: i64) {
    alert_dialog("Delete product?")
        .description("This removes it from the catalogue. This can't be undone.")
        .confirm("Delete")
        .cancel("Cancel")
        .destructive(true)
        .dismissible(true)
        .on_confirm(move || {
            store::delete_product(id);
            close_sheet(0);
        })
        .open();
}

fn order_history(product_id: i64) -> AnyWidget {
    let c = theme().colors;
    let rows: Vec<AnyWidget> = store::orders()
        .into_iter()
        .filter_map(|o| {
            o.items.iter().find(|l| l.product_id == product_id).map(|l| {
                let qty = l.qty;
                let code = o.code.clone();
                let date = o.date.clone();
                let status = o.status;
                container()
                    .padding(EdgeInsets::symmetric(0.0, 7.0))
                    .child(
                        row(children![
                            text(code).size(13.0).weight(600.0).color(c.foreground),
                            gap_w(10.0),
                            text(date).size(12.5).color(c.muted_foreground),
                            spacer(),
                            components::order_badge(status),
                            gap_w(10.0),
                            text(format!("×{qty}")).size(13.0).color(c.foreground),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Center),
                    )
                    .into_widget()
            })
        })
        .collect();

    if rows.is_empty() {
        return text("No orders yet for this product.").size(13.0).color(c.muted_foreground).into_widget();
    }
    column(rows)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
}

// ===========================================================================
// Order detail
// ===========================================================================

pub fn open_order_detail(id: i64) {
    sheet(component_props(order_view, id)).side(Side::Right).size(480.0).title("Order").open();
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn order_view(id: &i64) -> AnyWidget {
    let id = *id;
    let c = theme().colors;

    let Some(o) = store::order(id) else {
        return empty_sheet("This order no longer exists.");
    };
    let customer = store::customer(o.customer_id);
    let cust_name = customer.as_ref().map(|c| c.name.clone()).unwrap_or_else(|| "—".into());

    // Header: code, customer, date, status.
    let header = column(children![
        row(children![
            text(o.code.clone()).size(20.0).weight(700.0).color(c.foreground),
            spacer(),
            components::order_badge(o.status),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center),
        gap_h(4.0),
        text(format!("{cust_name} · {}", o.date)).size(13.0).color(c.muted_foreground),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .main_axis_size(MainAxisSize::Min);

    // Line items.
    let mut lines: Vec<AnyWidget> = o
        .items
        .iter()
        .map(|l| {
            row(children![
                Expanded::new(
                    column(children![
                        text(l.name.clone()).size(13.5).weight(500.0).color(c.foreground),
                        text(format!("{} × {}", l.qty, components::price(l.unit_cents)))
                            .size(12.0)
                            .color(c.muted_foreground),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
                text(components::price(l.line_total_cents())).size(13.5).weight(600.0).color(c.foreground),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .into_widget()
        })
        .collect();
    lines.insert(0, section_label("Items"));

    // Totals.
    let subtotal = o.subtotal_cents();
    let tax = (subtotal as f64 * store::settings().tax_rate / 100.0).round() as i64;
    let total = subtotal + tax;
    let totals = column(children![
        total_row("Subtotal", &components::price(subtotal), false),
        total_row(&format!("Tax ({:.1}%)", store::settings().tax_rate), &components::price(tax), false),
        gap_h(6.0),
        total_row("Total", &components::price(total), true),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .main_axis_size(MainAxisSize::Min);

    // Status changer — a select over every status.
    let labels: Vec<String> = OrderStatus::all().iter().map(|s| s.label().to_string()).collect();
    let current = OrderStatus::all().iter().position(|s| *s == o.status).unwrap_or(0);
    let status_select = select(labels).value(current).width(200.0).on_changed(move |i, _| {
        set_status(id, i);
    });

    // Fulfilment timeline.
    let timeline = timeline_widget(&o);

    let body: Vec<AnyWidget> = vec![
        header.into_widget(),
        gap_h(18.0).into_widget(),
        divider(),
        column(lines)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min)
            .into_widget(),
        gap_h(14.0).into_widget(),
        divider(),
        totals.into_widget(),
        gap_h(16.0).into_widget(),
        divider(),
        section_label("Update status"),
        status_select.into_widget(),
        gap_h(16.0).into_widget(),
        divider(),
        section_label("Fulfilment"),
        timeline,
        gap_h(16.0).into_widget(),
    ];

    scroll_view(
        column(body).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min),
    )
    .drag_scroll(true)
    .into_widget()
}

fn set_status(id: i64, index: usize) {
    let status = OrderStatus::all().get(index).copied().unwrap_or(OrderStatus::Pending);
    store::set_order_status(id, status);
}

fn timeline_widget(o: &Order) -> AnyWidget {
    let c = theme().colors;
    let rows: Vec<AnyWidget> = o
        .shipping
        .iter()
        .map(|ev| {
            let (dot, label_color) =
                if ev.done { (c.primary, c.foreground) } else { (c.border, c.muted_foreground) };
            container()
                .padding(EdgeInsets::symmetric(0.0, 7.0))
                .child(
                    row(children![
                        container()
                            .decoration(BoxDecoration::new().color(dot).shape(BoxShape::Circle))
                            .width(10.0)
                            .height(10.0),
                        gap_w(12.0),
                        text(ev.label.clone()).size(13.5).color(label_color),
                        spacer(),
                        text(ev.date.clone()).size(12.0).color(c.muted_foreground),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Center),
                )
                .into_widget()
        })
        .collect();
    column(rows)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
}

// ===========================================================================
// Customer detail
// ===========================================================================

pub fn open_customer_detail(id: i64) {
    sheet(component_props(customer_view, id)).side(Side::Right).size(460.0).title("Customer").open();
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn customer_view(id: &i64) -> AnyWidget {
    let id = *id;
    let c = theme().colors;

    let Some(cust) = store::customer(id) else {
        return empty_sheet("This customer no longer exists.");
    };

    let initials: String = cust.name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect();
    let header = row(children![
        container()
            .decoration(BoxDecoration::new().color(palette::violet::S500).shape(BoxShape::Circle))
            .width(56.0)
            .height(56.0)
            .alignment(Alignment::CENTER)
            .child(text(initials).size(20.0).weight(700.0).color(Color::WHITE)),
        gap_w(14.0),
        column(children![
            text(cust.name.clone()).size(18.0).weight(700.0).color(c.foreground),
            text(cust.company.clone()).size(13.0).color(c.muted_foreground),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center);

    let stats = row(children![
        metric("Orders", &store::customer_order_count(id).to_string()),
        metric("Spent", &components::price(store::customer_spent_cents(id))),
        metric("Since", &cust.since),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Start);

    let contact = column(children![
        contact_row(lucide::MAIL, &cust.email),
        gap_h(8.0),
        contact_row(lucide::PHONE, &cust.phone),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .main_axis_size(MainAxisSize::Min);

    // The customer's orders (tap to open the order sheet).
    let orders = store::orders_for_customer(id);
    let order_rows: Vec<AnyWidget> = if orders.is_empty() {
        vec![text("No orders yet.").size(13.0).color(c.muted_foreground).into_widget()]
    } else {
        orders
            .iter()
            .map(|o| {
                let oid = o.id;
                let code = o.code.clone();
                let date = o.date.clone();
                let status = o.status;
                let total = components::price(o.subtotal_cents());
                pressable(
                    container().padding(EdgeInsets::symmetric(0.0, 8.0)).child(
                        row(children![
                            text(code).size(13.5).weight(600.0).color(c.foreground),
                            gap_w(10.0),
                            text(date).size(12.5).color(c.muted_foreground),
                            spacer(),
                            components::order_badge(status),
                            gap_w(10.0),
                            text(total).size(13.0).color(c.foreground),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Center),
                    ),
                )
                .radius(8.0)
                .on_tap(move || {
                    close_sheet(0);
                    open_order_detail(oid);
                })
                .into_widget()
            })
            .collect()
    };

    let body: Vec<AnyWidget> = vec![
        header.into_widget(),
        gap_h(18.0).into_widget(),
        stats.into_widget(),
        gap_h(16.0).into_widget(),
        divider(),
        section_label("Contact"),
        contact.into_widget(),
        gap_h(16.0).into_widget(),
        divider(),
        section_label("Orders"),
        column(order_rows)
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min)
            .into_widget(),
        gap_h(16.0).into_widget(),
    ];

    scroll_view(
        column(body).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min),
    )
    .drag_scroll(true)
    .into_widget()
}

// ===========================================================================
// Shared bits
// ===========================================================================

fn metric(label: &str, value: &str) -> AnyWidget {
    let c = theme().colors;
    Expanded::new(
        column(children![
            text(value.to_string()).size(16.0).weight(700.0).color(c.foreground),
            gap_h(2.0),
            text(label.to_string()).size(12.0).color(c.muted_foreground),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min),
    )
    .into_widget()
}

fn contact_row(ic: IconData, value: &str) -> impl IntoWidget {
    let c = theme().colors;
    row(children![
        icon(ic).size(16.0).color(c.muted_foreground),
        gap_w(10.0),
        text(value.to_string()).size(13.5).color(c.foreground),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
}

fn total_row(label: &str, value: &str, strong: bool) -> AnyWidget {
    let c = theme().colors;
    let (size, weight) = if strong { (15.0, 700.0) } else { (13.5, 500.0) };
    row(children![
        text(label.to_string()).size(size).weight(weight).color(c.foreground),
        spacer(),
        text(value.to_string()).size(size).weight(weight).color(c.foreground),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .into_widget()
}

fn section_label(label: &str) -> AnyWidget {
    let c = theme().colors;
    container()
        .padding(EdgeInsets::only(0.0, 8.0, 0.0, 8.0))
        .child(text(label.to_string()).size(12.5).weight(700.0).color(c.muted_foreground))
        .into_widget()
}

fn divider() -> AnyWidget {
    container().color(theme().colors.border).height(1.0).margin(EdgeInsets::symmetric(0.0, 4.0)).into_widget()
}

fn empty_sheet(msg: &str) -> AnyWidget {
    let c = theme().colors;
    container()
        .padding(EdgeInsets::all(24.0))
        .child(text(msg.to_string()).size(14.0).color(c.muted_foreground))
        .into_widget()
}

// --- money <-> dollars string helpers for the edit form --------------------

fn dollars(cents: i64) -> String {
    format!("{}.{:02}", cents / 100, (cents % 100).abs())
}
fn parse_dollars(s: &str) -> i64 {
    let v: f64 = s.trim().trim_start_matches('$').parse().unwrap_or(0.0);
    (v * 100.0).round() as i64
}
