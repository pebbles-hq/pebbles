//! **Northwind** — a complete, offline inventory / sales desktop app built to show
//! what Pebbles can do on the desktop: a collapsible/hideable side nav, a top bar with
//! a center command-search and a notifications popover, rich data tables (search,
//! filters, sorting, status badges, pagination), right-hand detail sheets, and a
//! comprehensive settings form — all backed by a local SQLite database, fully offline,
//! with an optional cloud sync.

mod db;
mod model;
mod net;
mod screens;
mod sheets;
mod store;
mod ui;

use pebbles::prelude::*;

use sheets::{open_customer_detail, open_order_detail, open_product_detail};
use store::{NotifKind, Section};

fn app() -> impl IntoWidget {
    // Open the DB + load everything, once, on mount.
    create_effect(store::init);

    let c = theme().colors;
    let section = store::section();

    let mut shell = scaffold(content(section)).top(top_bar(section)).background(c.background);
    if store::nav_visible() {
        shell = shell.side(side_panel(section));
    }
    shell
}

fn content(section: Section) -> AnyWidget {
    match section {
        Section::Dashboard => screens::dashboard().into_widget(),
        Section::Products => screens::products().into_widget(),
        Section::Orders => screens::orders().into_widget(),
        Section::Customers => screens::customers().into_widget(),
        Section::Settings => screens::settings().into_widget(),
    }
}

// ===========================================================================
// Top bar — hamburger · title · center search · sync · notifications · avatar
// ===========================================================================

fn top_bar(section: Section) -> AnyWidget {
    let c = theme().colors;
    let bar = container().color(c.card).height(58.0).padding(EdgeInsets::symmetric(14.0, 0.0)).child(
        row(children![
            icon_button(lucide::PANEL_LEFT).variant(ButtonVariant::Ghost).on_pressed(store::toggle_nav),
            gap_w(8.0),
            text(section.title()).size(16.0).weight(700.0).color(c.foreground),
            Expanded::new(center(search_box())),
            sync_button(),
            gap_w(8.0),
            notif_bell(),
            gap_w(12.0),
            avatar("RS").color(palette::violet::S500),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center),
    );
    column(children![bar, container().color(c.border).height(1.0)])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
}

/// The center search field — looks like an input, opens the ⌘K command palette.
fn search_box() -> impl IntoWidget {
    let c = theme().colors;
    pressable(
        container()
            .decoration(
                BoxDecoration::new()
                    .color(c.background)
                    .border(Border::new(c.border, 1.0))
                    .radius(BorderRadius::all(10.0)),
            )
            .width(440.0)
            .padding(EdgeInsets::symmetric(12.0, 8.0))
            .child(
                row(children![
                    icon(lucide::SEARCH).size(16.0).color(c.muted_foreground),
                    gap_w(8.0),
                    text("Search products, orders, customers…").size(13.0).color(c.muted_foreground),
                    spacer(),
                    kbd_hint("⌘K"),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Center),
            ),
    )
    .radius(10.0)
    .on_tap(open_search)
}

fn kbd_hint(s: &str) -> impl IntoWidget {
    let c = theme().colors;
    container()
        .decoration(BoxDecoration::new().color(c.secondary).radius(BorderRadius::all(6.0)))
        .padding(EdgeInsets::symmetric(6.0, 2.0))
        .child(text(s.to_string()).size(11.0).color(c.muted_foreground))
}

/// Build a command palette over every product / order / customer and open it.
fn open_search() {
    let products: Vec<CommandItem> = store::products()
        .iter()
        .take(80)
        .map(|p| {
            let id = p.id;
            command_item(format!("{} · {}", p.name, p.sku)).icon(lucide::PACKAGE).on_select(move || {
                store::go_to(Section::Products);
                open_product_detail(id);
            })
        })
        .collect();
    let orders: Vec<CommandItem> = store::orders()
        .iter()
        .take(80)
        .map(|o| {
            let id = o.id;
            let name = store::customer(o.customer_id).map(|c| c.name).unwrap_or_default();
            command_item(format!("{} · {}", o.code, name)).icon(lucide::SHOPPING_CART).on_select(move || {
                store::go_to(Section::Orders);
                open_order_detail(id);
            })
        })
        .collect();
    let customers: Vec<CommandItem> = store::customers()
        .iter()
        .map(|cu| {
            let id = cu.id;
            command_item(format!("{} · {}", cu.name, cu.company)).icon(lucide::USERS).on_select(move || {
                store::go_to(Section::Customers);
                open_customer_detail(id);
            })
        })
        .collect();

    command_palette(vec![
        command_group("Products", products),
        command_group("Orders", orders),
        command_group("Customers", customers),
    ])
    .placeholder("Search products, orders, customers…")
    .width(620.0)
    .open();
}

fn sync_button() -> AnyWidget {
    let c = theme().colors;
    if store::syncing() {
        container()
            .padding(EdgeInsets::symmetric(10.0, 6.0))
            .child(
                row(children![
                    spinner(15.0).color(c.muted_foreground),
                    gap_w(7.0),
                    text("Syncing…").size(13.0).color(c.muted_foreground),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_size(MainAxisSize::Min),
            )
            .into_widget()
    } else {
        button("Sync")
            .variant(ButtonVariant::Secondary)
            .leading(lucide::CLOUD_DOWNLOAD)
            .on_pressed(store::sync_from_cloud)
            .into_widget()
    }
}

// ===========================================================================
// Notifications popover
// ===========================================================================

fn notif_bell() -> AnyWidget {
    let c = theme().colors;
    let unread = store::unread_notifs();
    let glyph: AnyWidget = if unread > 0 {
        stack(children![
            icon(lucide::BELL).size(20.0).color(c.foreground),
            positioned(
                container()
                    .decoration(BoxDecoration::new().color(palette::rose::S500).shape(BoxShape::Circle))
                    .width(8.0)
                    .height(8.0),
            )
            .right(0.0)
            .top(0.0),
        ])
        .into_widget()
    } else {
        icon(lucide::BELL).size(20.0).color(c.foreground).into_widget()
    };
    let trigger = container().padding(EdgeInsets::all(8.0)).child(glyph);
    // pad(0) so the header divider and row separators run edge to edge.
    popover(notif_panel(), trigger).width(360.0).height(420.0).trigger_height(38.0).pad(0.0).into_widget()
}

fn notif_panel() -> AnyWidget {
    let c = theme().colors;
    let notifs = store::notifications();
    let unread = store::unread_notifs();

    // Header: title (+ unread count) and "Mark all read" when there's something unread.
    let title = if unread > 0 { format!("Notifications · {unread}") } else { "Notifications".to_string() };
    let mut head: Vec<AnyWidget> =
        vec![text(title).size(14.0).weight(700.0).color(c.foreground).into_widget(), spacer().into_widget()];
    if unread > 0 {
        head.push(
            pressable(text("Mark all read").size(12.5).weight(500.0).color(c.primary))
                .radius(6.0)
                .on_tap(store::mark_notifs_read)
                .into_widget(),
        );
    }
    let header = container()
        .padding(EdgeInsets::symmetric(14.0, 12.0))
        .child(row(head).cross_axis_alignment(CrossAxisAlignment::Center));

    // Body: rows separated by hairlines, or an empty state. Sizes to content for a few
    // notifications; caps to a scrollable height when there are many.
    let body: AnyWidget = if notifs.is_empty() {
        container()
            .padding(EdgeInsets::symmetric(14.0, 36.0))
            .alignment(Alignment::CENTER)
            .child(text("You're all caught up ✦").size(13.0).color(c.muted_foreground))
            .into_widget()
    } else {
        let mut kids: Vec<AnyWidget> = Vec::new();
        for (i, n) in notifs.iter().enumerate() {
            if i > 0 {
                kids.push(container().color(c.border).height(1.0).into_widget());
            }
            kids.push(notif_row(n));
        }
        let list =
            column(kids).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min);
        if notifs.len() > 6 {
            container().height(360.0).child(scroll_view(list).drag_scroll(true)).into_widget()
        } else {
            list.into_widget()
        }
    };

    column(children![header, container().color(c.border).height(1.0), body])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
}

fn notif_row(n: &store::Notif) -> AnyWidget {
    let c = theme().colors;
    let (ic, color) = match n.kind {
        NotifKind::LowStock => (lucide::PACKAGE, palette::amber::S500),
        NotifKind::Order => (lucide::SHOPPING_CART, palette::sky::S500),
        NotifKind::Sync => (lucide::CLOUD_DOWNLOAD, palette::emerald::S500),
        NotifKind::Info => (lucide::BELL, palette::violet::S500),
    };
    let unread = !n.read;
    // Unread rows get a faint accent wash across the full width.
    let bg = if unread { ui::mix(c.card, c.accent, 0.5) } else { palette::TRANSPARENT };
    // Trailing dot marks unread rows (kept as a gap when read, to hold alignment).
    let dot: AnyWidget = if unread {
        container()
            .decoration(BoxDecoration::new().color(c.primary).shape(BoxShape::Circle))
            .width(7.0)
            .height(7.0)
            .into_widget()
    } else {
        gap_h(7.0).into_widget()
    };

    container()
        .color(bg)
        .padding(EdgeInsets::symmetric(14.0, 11.0))
        .child(
            row(children![
                container()
                    .decoration(
                        BoxDecoration::new()
                            .color(ui::mix(c.card, color, 0.16))
                            .radius(BorderRadius::all(9.0)),
                    )
                    .padding(EdgeInsets::all(8.0))
                    .child(icon(ic).size(15.0).color(color)),
                gap_w(11.0),
                Expanded::new(
                    column(children![
                        text(n.title.clone()).size(13.0).weight(600.0).color(c.foreground),
                        gap_h(2.0),
                        text(n.body.clone()).size(12.0).line_height(1.4).color(c.muted_foreground),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
                gap_w(10.0),
                column(children![text(n.time.clone()).size(11.0).color(c.muted_foreground), gap_h(6.0), dot])
                    .cross_axis_alignment(CrossAxisAlignment::End)
                    .main_axis_size(MainAxisSize::Min),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start),
        )
        .into_widget()
}

// ===========================================================================
// Side navigation
// ===========================================================================

fn side_panel(section: Section) -> AnyWidget {
    side_nav()
        .width(238.0)
        .collapsible(true)
        .collapsed(store::nav_collapsed())
        .on_collapse_changed(store::set_nav_collapsed)
        .header(brand())
        .item(nav(lucide::LAYOUT_DASHBOARD, "Dashboard", Section::Dashboard, section))
        .item(nav(lucide::PACKAGE, "Products", Section::Products, section))
        .item(nav(lucide::SHOPPING_CART, "Orders", Section::Orders, section))
        .item(nav(lucide::USERS, "Customers", Section::Customers, section))
        .item(nav(lucide::SETTINGS, "Settings", Section::Settings, section))
        .footer(profile())
        .into_widget()
}

fn nav(ic: IconData, label: &str, target: Section, current: Section) -> NavItem {
    nav_item(label).icon(ic).selected(current == target).on_select(move || store::go_to(target))
}

fn brand() -> impl IntoWidget {
    let c = theme().colors;
    row(children![
        container()
            .decoration(BoxDecoration::new().color(c.primary).radius(BorderRadius::all(8.0)))
            .padding(EdgeInsets::all(6.0))
            .child(icon(lucide::BOXES).size(18.0).color(c.primary_foreground)),
        gap_w(10.0),
        text("Northwind").size(16.0).weight(700.0).color(c.foreground),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
}

fn profile() -> impl IntoWidget {
    let c = theme().colors;
    row(children![
        avatar("RS").color(palette::violet::S500),
        gap_w(10.0),
        column(children![
            text("Reyco").size(13.5).semibold().color(c.foreground),
            text("Administrator").size(12.0).color(c.muted_foreground),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
}

#[pebbles::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(component(app)).title("Northwind — Inventory").size(1280, 840).run()
}
