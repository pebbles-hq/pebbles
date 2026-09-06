//! **Northwind** — a complete, offline inventory / sales desktop app built to show
//! what Pebbles can do on the desktop: a collapsible/hideable side nav, a top bar with
//! a center command-search and a notifications popover, rich data tables (search,
//! filters, sorting, status badges, pagination), right-hand detail sheets, and a
//! comprehensive settings form — all backed by a local SQLite database, fully offline,
//! with an optional cloud sync.

mod components;
mod db;
mod model;
mod net;
mod screens;
mod sheets;
mod store;

use pebbles::prelude::*;

use sheets::{open_customer_detail, open_order_detail, open_product_detail};
use store::Section;

fn app() -> impl IntoWidget {
    // Open the DB + load everything, once, on mount.
    create_effect(store::init);

    let c = theme().colors;
    let section = store::section();
    // Responsive: a persistent side nav on desktop; on tablet/mobile it's hidden and
    // the hamburger opens it as a floating left drawer instead (and the top-bar search
    // collapses to an icon). `breakpoint()` is reactive, so resizing re-lays out live.
    let desktop = breakpoint().select(false, false, true);

    let mut shell = scaffold(content(section)).top(top_bar(section, desktop)).background(c.background);
    if desktop && store::nav_visible() {
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

fn top_bar(section: Section, desktop: bool) -> AnyWidget {
    let c = theme().colors;
    // The hamburger toggles the persistent nav on desktop, opens the drawer otherwise.
    let menu = icon_button(lucide::PANEL_LEFT).variant(ButtonVariant::Ghost).on_pressed(move || {
        if desktop {
            store::toggle_nav();
        } else {
            open_nav_drawer(section);
        }
    });

    let mut kids: Vec<AnyWidget> = vec![
        menu.into_widget(),
        gap_w(8.0).into_widget(),
        text(section.title()).size(16.0).weight(700.0).color(c.foreground).into_widget(),
    ];
    if desktop {
        // Full center search + labelled Sync.
        kids.push(Expanded::new(center(search_box())).into_widget());
        kids.push(sync_button(false));
        kids.push(gap_w(8.0).into_widget());
    } else {
        // Collapse the search to an icon and the Sync to an icon; push actions right.
        kids.push(spacer().into_widget());
        kids.push(
            icon_button(lucide::SEARCH).variant(ButtonVariant::Ghost).on_pressed(open_search).into_widget(),
        );
        kids.push(gap_w(2.0).into_widget());
        kids.push(sync_button(true));
        kids.push(gap_w(2.0).into_widget());
    }
    kids.push(components::notifications_button());
    kids.push(gap_w(10.0).into_widget());
    kids.push(avatar("RS").color(palette::violet::S500).into_widget());

    let bar = container()
        .color(c.card)
        .height(58.0)
        .padding(EdgeInsets::symmetric(14.0, 0.0))
        .child(row(kids).cross_axis_alignment(CrossAxisAlignment::Center));
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

fn sync_button(compact: bool) -> AnyWidget {
    let c = theme().colors;
    if store::syncing() {
        let mut kids: Vec<AnyWidget> = vec![spinner(15.0).color(c.muted_foreground).into_widget()];
        if !compact {
            kids.push(gap_w(7.0).into_widget());
            kids.push(text("Syncing…").size(13.0).color(c.muted_foreground).into_widget());
        }
        container()
            .padding(EdgeInsets::symmetric(10.0, 6.0))
            .child(
                row(kids).cross_axis_alignment(CrossAxisAlignment::Center).main_axis_size(MainAxisSize::Min),
            )
            .into_widget()
    } else if compact {
        icon_button(lucide::CLOUD_DOWNLOAD)
            .variant(ButtonVariant::Ghost)
            .on_pressed(store::sync_from_cloud)
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

/// Open the side nav as a floating **left drawer** (a left sheet over the content) —
/// the tablet/mobile behavior. Zero host padding so the nav runs edge to edge.
fn open_nav_drawer(section: Section) {
    sheet(nav_drawer(section)).side(Side::Left).size(280.0).padding(EdgeInsets::ZERO).open();
}

/// The nav rendered inside the drawer sheet: full window height so the footer sits at
/// the bottom, and tapping an item navigates AND closes the drawer.
fn nav_drawer(section: Section) -> AnyWidget {
    let h = media_query().size.height.max(320.0);
    container()
        .height(h)
        .child(
            side_nav()
                .width(280.0)
                .header(brand())
                .item(nav_go(lucide::LAYOUT_DASHBOARD, "Dashboard", Section::Dashboard, section))
                .item(nav_go(lucide::PACKAGE, "Products", Section::Products, section))
                .item(nav_go(lucide::SHOPPING_CART, "Orders", Section::Orders, section))
                .item(nav_go(lucide::USERS, "Customers", Section::Customers, section))
                .item(nav_go(lucide::SETTINGS, "Settings", Section::Settings, section))
                .footer(profile()),
        )
        .into_widget()
}

fn nav_go(ic: IconData, label: &str, target: Section, current: Section) -> NavItem {
    nav_item(label).icon(ic).selected(current == target).on_select(move || {
        store::go_to(target);
        close_sheet(0);
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use pebbles_testing::Harness;

    fn blank() -> impl IntoWidget {
        container()
    }

    /// The hamburger's tablet/mobile behavior: the side nav opens as a floating left
    /// sheet with its items rendered.
    #[test]
    fn nav_drawer_opens_as_a_left_sheet() {
        let mut h = Harness::new().window(500.0, 840.0);
        store::init();
        h.mount(blank);
        h.settle();
        let base = h.element_count();

        open_nav_drawer(Section::Products);
        h.settle();
        h.draw();

        assert!(
            h.element_count() > base + 10,
            "the nav drawer should render its items (base {base}, now {})",
            h.element_count()
        );
    }

    /// Responsive shell: desktop carries the persistent side nav (and the full search),
    /// so it has materially more chrome than the narrow layout (drawer + icon search).
    #[test]
    fn desktop_has_persistent_sidebar_narrow_does_not() {
        let mut wide = Harness::new().window(1280.0, 840.0);
        pebbles_widgets::overlay::set_window_size(1280.0, 840.0);
        store::init();
        wide.mount(app);
        wide.settle();
        let desktop_count = wide.element_count();

        let mut narrow = Harness::new().window(600.0, 840.0);
        pebbles_widgets::overlay::set_window_size(600.0, 840.0);
        narrow.mount(app);
        narrow.settle();
        let narrow_count = narrow.element_count();

        assert!(
            desktop_count > narrow_count,
            "desktop (persistent sidebar + full search) should have more chrome than narrow \
             (drawer + icon search): {desktop_count} vs {narrow_count}"
        );
    }
}
