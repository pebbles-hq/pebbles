//! The right-hand **detail sheets** — comprehensive drill-downs opened from a table
//! row (or the global search). Each reads the store reactively, so edits made here
//! (stock, status) update the underlying table live.

use pebbles::prelude::*;

use crate::components;
use crate::model::{self, Order, OrderStatus};
use crate::store;

// ===========================================================================
// Product detail
// ===========================================================================

pub fn open_product_detail(id: i64) {
    // Wider than the other sheets — the product form is a full management surface
    // (media, details, pricing, inventory, history), so it needs the room.
    sheet(component_props(product_view, id)).side(Side::Right).size(600.0).title("Product").open();
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn product_view(id: &i64) -> AnyWidget {
    let id = *id;
    let c = theme().colors;
    let selected = create_signal(0_usize); // gallery preview selection
    let new_image = create_signal(String::new());

    let Some(p) = store::product(id) else {
        return empty_sheet("This product no longer exists.");
    };

    // --- form state: seeded once, persisted together on Save -----------------
    let name = create_signal(p.name.clone());
    let sku = create_signal(p.sku.clone());
    let brand = create_signal(p.brand.clone());
    let description = create_signal(p.description.clone());
    let price = create_signal(dollars(p.price_cents));
    let cost = create_signal(dollars(p.cost_cents));
    let reorder = create_signal(p.reorder_level.to_string());

    // Category picker options: the catalogue's categories + this product's own.
    let mut cats = store::categories();
    if !cats.contains(&p.category) {
        cats.push(p.category.clone());
        cats.sort();
    }
    let cat_start = cats.iter().position(|x| *x == p.category).unwrap_or(0);
    let cat_idx = create_signal(cat_start);

    // === Media: hero preview + manageable thumbnails =========================
    let sel = selected.get().min(p.images.len().saturating_sub(1));
    let hero: AnyWidget = match p.images.get(sel) {
        Some(url) => container()
            .decoration(BoxDecoration::new().color(c.secondary).radius(BorderRadius::all(14.0)))
            .clip()
            .height(240.0)
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
            .height(240.0)
            .alignment(Alignment::CENTER)
            .child(icon(lucide::PACKAGE).size(40.0).color(c.muted_foreground))
            .into_widget(),
    };

    // "Set as cover" appears while previewing a non-cover image.
    let cover_action: AnyWidget = if p.images.len() > 1 && sel != 0 {
        container()
            .padding(EdgeInsets::only(0.0, 8.0, 0.0, 0.0))
            .child(
                button("Set as cover").variant(ButtonVariant::Ghost).leading(lucide::IMAGE_PLUS).on_pressed(
                    move || {
                        store::set_cover_image(id, sel);
                        selected.set(0);
                    },
                ),
            )
            .into_widget()
    } else {
        gap_h(0.0).into_widget()
    };

    // Thumbnails: tap to preview, × to remove.
    let thumbs: Vec<AnyWidget> = p
        .images
        .iter()
        .enumerate()
        .map(|(i, url)| {
            let active = i == sel;
            let border = if active { c.primary } else { c.border };
            let tile = pressable(
                container()
                    .decoration(
                        BoxDecoration::new()
                            .color(c.secondary)
                            .border(Border::new(border, if active { 2.0 } else { 1.0 }))
                            .radius(BorderRadius::all(9.0)),
                    )
                    .clip()
                    .width(60.0)
                    .height(60.0)
                    .child(ImageView::network(url.clone()).fit(ImageFit::Cover)),
            )
            .radius(9.0)
            .on_tap(move || selected.set(i));
            let remove = positioned(
                pressable(
                    container()
                        .decoration(BoxDecoration::new().color(c.foreground).shape(BoxShape::Circle))
                        .width(18.0)
                        .height(18.0)
                        .alignment(Alignment::CENTER)
                        .child(icon(lucide::X).size(11.0).color(c.background)),
                )
                .radius(9.0)
                .on_tap(move || {
                    store::remove_product_image(id, i);
                    selected.set(0);
                }),
            )
            .right(3.0)
            .top(3.0);
            stack(children![tile, remove]).into_widget()
        })
        .collect();

    let add_image = row(children![
        Expanded::new(text_field().leading(lucide::LINK).placeholder("Paste an image URL…").bind(new_image),),
        gap_w(10.0),
        button("Add image").variant(ButtonVariant::Outline).leading(lucide::IMAGE_PLUS).on_pressed(
            move || {
                let url = new_image.peek();
                if !url.trim().is_empty() {
                    store::add_product_image(id, url);
                    new_image.set(String::new());
                }
            },
        ),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center);

    // === Header ==============================================================
    let header = column(children![
        row(children![
            Expanded::new(text(p.name.clone()).size(20.0).weight(700.0).max_lines(2).color(c.foreground)),
            gap_w(10.0),
            components::stock_badge(&p),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start),
        gap_h(4.0),
        row(children![
            text(format!("SKU {} · {}", p.sku, p.brand)).size(13.0).color(c.muted_foreground),
            spacer(),
            components::stars(p.rating),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .main_axis_size(MainAxisSize::Min);

    // Summary metrics (live, from the stored product).
    let metrics = row(children![
        metric("Price", &components::price(p.price_cents)),
        metric("Margin", &components::price(p.margin_cents())),
        metric("Stock value", &components::price(p.stock_value_cents())),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Start);

    // === Inventory: live quick actions (persist immediately) =================
    let stock_color = match p.status() {
        model::StockStatus::OutOfStock => palette::rose::S500,
        model::StockStatus::LowStock => palette::amber::S500,
        model::StockStatus::InStock => c.foreground,
    };
    let stock_stepper = row(children![
        column(children![
            text("On hand").size(12.5).color(c.muted_foreground),
            gap_h(2.0),
            text(p.stock.to_string()).size(24.0).weight(700.0).color(stock_color),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min),
        spacer(),
        icon_button(lucide::MINUS)
            .variant(ButtonVariant::Outline)
            .on_pressed(move || store::adjust_stock(id, -1)),
        gap_w(8.0),
        icon_button(lucide::PLUS)
            .variant(ButtonVariant::Outline)
            .on_pressed(move || store::adjust_stock(id, 1)),
        gap_w(12.0),
        button("Restock")
            .variant(ButtonVariant::Secondary)
            .leading(lucide::REFRESH_CW)
            .on_pressed(move || store::reorder(id)),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center);

    let stock_meta = row(children![
        components::stock_badge(&p),
        spacer(),
        text(format!("Reorder at {}", p.reorder_level)).size(12.5).color(c.muted_foreground),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center);

    // A live margin preview that recomputes as the price/cost fields are typed —
    // a small nested component so only this line re-runs, not the whole sheet.
    let margin_line = component_props(margin_preview, (price, cost)).into_widget();

    // Save the whole descriptive/pricing/rules form at once.
    let cats_save = cats.clone();
    let save = move || {
        let category = cats_save.get(cat_idx.peek()).cloned().unwrap_or_default();
        store::save_product(
            id,
            store::ProductEdits {
                name: name.peek(),
                sku: sku.peek(),
                brand: brand.peek(),
                category,
                description: description.peek(),
                price_cents: parse_dollars(&price.peek()),
                cost_cents: parse_dollars(&cost.peek()),
                reorder_level: reorder.peek().trim().parse::<i64>().unwrap_or(0),
            },
        );
    };

    // Recent orders that included this product — the item's "history".
    let history = order_history(id);

    let body: Vec<AnyWidget> = vec![
        hero,
        cover_action,
        gap_h(10.0).into_widget(),
        row(thumbs).main_axis_size(MainAxisSize::Min).into_widget(),
        gap_h(10.0).into_widget(),
        add_image.into_widget(),
        gap_h(18.0).into_widget(),
        header.into_widget(),
        gap_h(16.0).into_widget(),
        metrics.into_widget(),
        gap_h(16.0).into_widget(),
        divider(),
        section_label("Inventory"),
        stock_stepper.into_widget(),
        gap_h(10.0).into_widget(),
        stock_meta.into_widget(),
        gap_h(16.0).into_widget(),
        divider(),
        section_label("Details"),
        field(text_field().bind(name)).label("Name").into_widget(),
        gap_h(10.0).into_widget(),
        field(text_field().bind(sku)).label("SKU").into_widget(),
        gap_h(10.0).into_widget(),
        row(children![
            Expanded::new(field(text_field().bind(brand)).label("Brand")),
            gap_w(10.0),
            Expanded::new(
                field(select(cats.clone()).value(cat_start).on_changed(move |i, _| cat_idx.set(i)))
                    .label("Category"),
            ),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .into_widget(),
        gap_h(10.0).into_widget(),
        field(text_area(4).bind(description)).label("Description").into_widget(),
        gap_h(16.0).into_widget(),
        divider(),
        section_label("Pricing & stock rules"),
        row(children![
            Expanded::new(field(text_field().bind(price).kind(InputKind::Number)).label("Price")),
            gap_w(10.0),
            Expanded::new(field(text_field().bind(cost).kind(InputKind::Number)).label("Cost")),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .into_widget(),
        gap_h(10.0).into_widget(),
        margin_line,
        gap_h(12.0).into_widget(),
        field(text_field().bind(reorder).kind(InputKind::Integer))
            .label("Reorder at")
            .description("Low-stock alerts fire at or below this quantity.")
            .into_widget(),
        gap_h(16.0).into_widget(),
        button("Save changes").leading(lucide::CHECK).on_pressed(save).into_widget(),
        gap_h(18.0).into_widget(),
        divider(),
        section_label("Order history"),
        history,
        gap_h(20.0).into_widget(),
        divider(),
        section_label("Danger zone"),
        row(children![
            Expanded::new(
                column(children![
                    text("Delete this product").size(13.5).weight(600.0).color(c.foreground),
                    gap_h(2.0),
                    text("Removes it from the catalogue. This can't be undone.")
                        .size(12.0)
                        .color(c.muted_foreground),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min),
            ),
            gap_w(10.0),
            button("Delete")
                .variant(ButtonVariant::Destructive)
                .leading(lucide::TRASH_2)
                .on_pressed(move || confirm_delete(id)),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .into_widget(),
        gap_h(12.0).into_widget(),
    ];

    scroll_view(
        column(body).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min),
    )
    .drag_scroll(true)
    .into_widget()
}

/// Live margin readout (amount + %) that recomputes as the price/cost fields change.
/// A nested component keyed on the two field signals, so typing a price re-runs only
/// this line — not the whole product sheet.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn margin_preview(props: &(Signal<String>, Signal<String>)) -> AnyWidget {
    let (price, cost) = *props;
    let c = theme().colors;
    let pr = parse_dollars(&price.get());
    let co = parse_dollars(&cost.get());
    let m = pr - co;
    let pct = if pr > 0 { (m as f64 / pr as f64) * 100.0 } else { 0.0 };
    row(children![
        text("Projected margin").size(12.5).color(c.muted_foreground),
        spacer(),
        text(format!("{}  ·  {pct:.0}%", components::price(m))).size(13.0).weight(600.0).color(if m >= 0 {
            c.foreground
        } else {
            palette::rose::S500
        }),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
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
    sheet(component_props(order_view, id)).side(Side::Right).size(520.0).title("Order").open();
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
