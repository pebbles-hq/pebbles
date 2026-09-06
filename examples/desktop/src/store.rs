//! The **state manager** — reactive signals over the SQLite database.
//!
//! On startup it opens the DB, seeds it with the embedded catalogue on first run, and
//! loads everything into app-scoped signals. Screens read those signals (and re-render
//! on change); every action mutates a signal **and** writes through to SQLite, so the
//! data survives a restart. The optional cloud sync pulls fresh products off the UI
//! thread and merges them the same way.

use std::cell::{Cell, RefCell};

use pebbles::prelude::*;

use crate::model::{
    self, Customer, Order, OrderStatus, Product, Settings, seed_customers, seed_orders, seed_products,
};
use crate::{db, net};

/// Cloud-synced products are stored under this id offset so they never collide with
/// the seeded (1–24) or locally-created catalogue.
const SYNC_BASE: i64 = 100_000;

// ---------------------------------------------------------------------------
// Notifications (the bell popover)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NotifKind {
    LowStock,
    Order,
    Sync,
    Info,
}

#[derive(Clone)]
pub struct Notif {
    pub kind: NotifKind,
    pub title: String,
    pub body: String,
    pub time: String,
    pub read: bool,
}

// ---------------------------------------------------------------------------
// Navigation sections
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Dashboard,
    Products,
    Orders,
    Customers,
    Settings,
}

impl Section {
    pub fn title(self) -> &'static str {
        match self {
            Section::Dashboard => "Dashboard",
            Section::Products => "Products",
            Section::Orders => "Orders",
            Section::Customers => "Customers",
            Section::Settings => "Settings",
        }
    }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

thread_local! {
    static PRODUCTS: RefCell<Option<Signal<Vec<Product>>>> = const { RefCell::new(None) };
    static CUSTOMERS: RefCell<Option<Signal<Vec<Customer>>>> = const { RefCell::new(None) };
    static ORDERS: RefCell<Option<Signal<Vec<Order>>>> = const { RefCell::new(None) };
    static SETTINGS: RefCell<Option<Signal<Settings>>> = const { RefCell::new(None) };
    static NOTIFS: RefCell<Option<Signal<Vec<Notif>>>> = const { RefCell::new(None) };
    static SECTION: RefCell<Option<Signal<Section>>> = const { RefCell::new(None) };
    static NAV_VISIBLE: RefCell<Option<Signal<bool>>> = const { RefCell::new(None) };
    static NAV_COLLAPSED: RefCell<Option<Signal<bool>>> = const { RefCell::new(None) };
    static SYNCING: RefCell<Option<Signal<bool>>> = const { RefCell::new(None) };
    static INITED: Cell<bool> = const { Cell::new(false) };
}

fn products_sig() -> Signal<Vec<Product>> {
    PRODUCTS.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(Vec::new())))
}
fn customers_sig() -> Signal<Vec<Customer>> {
    CUSTOMERS.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(Vec::new())))
}
fn orders_sig() -> Signal<Vec<Order>> {
    ORDERS.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(Vec::new())))
}
fn settings_sig() -> Signal<Settings> {
    SETTINGS.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(Settings::default())))
}
fn notifs_sig() -> Signal<Vec<Notif>> {
    NOTIFS.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(seed_notifs())))
}
fn section_sig() -> Signal<Section> {
    SECTION.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(Section::Dashboard)))
}
fn nav_visible_sig() -> Signal<bool> {
    NAV_VISIBLE.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(true)))
}
fn nav_collapsed_sig() -> Signal<bool> {
    NAV_COLLAPSED.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(false)))
}
fn syncing_sig() -> Signal<bool> {
    SYNCING.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(false)))
}

/// Open the database, seed it on first run, and load everything into the signals.
pub fn init() {
    if INITED.with(Cell::get) {
        return;
    }
    INITED.with(|c| c.set(true));
    db::init();

    if db::is_empty() {
        // First run: fill the DB with the embedded catalogue so it works offline now.
        let products = seed_products();
        let customers = seed_customers();
        let orders = seed_orders(&products, &customers);
        for p in &products {
            db::upsert_product(p);
        }
        for c in &customers {
            db::upsert_customer(c);
        }
        for o in &orders {
            db::upsert_order(o);
        }
        db::save_settings(&Settings::default());
    }

    products_sig().set(db::load_products());
    customers_sig().set(db::load_customers());
    orders_sig().set(db::load_orders());
    if let Some(s) = db::load_settings() {
        settings_sig().set(s);
    }

    // Apply the persisted dark-mode preference (theme starts light). Peek so init,
    // when driven from a mount effect, doesn't subscribe to the settings signal.
    if settings_sig().peek().dark_mode {
        toggle_theme();
    }
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

pub fn products() -> Vec<Product> {
    products_sig().get()
}
pub fn product(id: i64) -> Option<Product> {
    products_sig().get().into_iter().find(|p| p.id == id)
}
pub fn customers() -> Vec<Customer> {
    customers_sig().get()
}
pub fn customer(id: i64) -> Option<Customer> {
    customers_sig().get().into_iter().find(|c| c.id == id)
}
pub fn orders() -> Vec<Order> {
    orders_sig().get()
}
pub fn order(id: i64) -> Option<Order> {
    orders_sig().get().into_iter().find(|o| o.id == id)
}
pub fn settings() -> Settings {
    settings_sig().get()
}
pub fn notifications() -> Vec<Notif> {
    notifs_sig().get()
}
pub fn unread_notifs() -> usize {
    notifs_sig().get().iter().filter(|n| !n.read).count()
}
pub fn section() -> Section {
    section_sig().get()
}
pub fn nav_visible() -> bool {
    nav_visible_sig().get()
}
pub fn nav_collapsed() -> bool {
    nav_collapsed_sig().get()
}
pub fn syncing() -> bool {
    syncing_sig().get()
}

/// The currency symbol from settings (reactive — used by every money label).
pub fn symbol() -> String {
    settings().currency_symbol().to_string()
}

// --- derived (customers) ----------------------------------------------------

/// How many orders a customer has placed.
pub fn customer_order_count(cid: i64) -> usize {
    orders_sig().get().iter().filter(|o| o.customer_id == cid).count()
}
/// A customer's lifetime spend (sum of non-cancelled order subtotals).
pub fn customer_spent_cents(cid: i64) -> i64 {
    orders_sig()
        .get()
        .iter()
        .filter(|o| o.customer_id == cid && o.status != OrderStatus::Cancelled)
        .map(|o| o.subtotal_cents())
        .sum()
}
/// Orders belonging to a customer (for their detail sheet).
pub fn orders_for_customer(cid: i64) -> Vec<Order> {
    orders_sig().get().into_iter().filter(|o| o.customer_id == cid).collect()
}

// --- derived (dashboard KPIs) ----------------------------------------------

pub fn inventory_value_cents() -> i64 {
    products_sig().get().iter().map(Product::stock_value_cents).sum()
}
pub fn low_stock() -> Vec<Product> {
    products_sig().get().into_iter().filter(|p| p.status() != model::StockStatus::InStock).collect()
}
pub fn revenue_cents() -> i64 {
    orders_sig()
        .get()
        .iter()
        .filter(|o| o.status != OrderStatus::Cancelled && o.status != OrderStatus::Pending)
        .map(|o| o.subtotal_cents())
        .sum()
}
pub fn pending_orders() -> usize {
    orders_sig().get().iter().filter(|o| o.status == OrderStatus::Pending).count()
}

// ---------------------------------------------------------------------------
// Navigation actions
// ---------------------------------------------------------------------------

pub fn go_to(section: Section) {
    section_sig().set(section);
}
pub fn toggle_nav() {
    nav_visible_sig().update(|v| *v = !*v);
}
pub fn set_nav_collapsed(collapsed: bool) {
    nav_collapsed_sig().set(collapsed);
}

// ---------------------------------------------------------------------------
// Product actions
// ---------------------------------------------------------------------------

/// The descriptive, pricing and stock-rule fields edited on the product sheet's form
/// (everything persisted together on **Save**; stock and images are separate live
/// quick-actions). Money is in integer cents.
#[derive(Clone)]
pub struct ProductEdits {
    pub name: String,
    pub sku: String,
    pub brand: String,
    pub category: String,
    pub description: String,
    pub price_cents: i64,
    pub cost_cents: i64,
    pub reorder_level: i64,
}

/// The distinct product categories in the catalogue, sorted — the category picker's
/// options on the product form.
pub fn categories() -> Vec<String> {
    let mut cats: Vec<String> = Vec::new();
    for p in products_sig().get() {
        if !cats.contains(&p.category) {
            cats.push(p.category);
        }
    }
    cats.sort();
    cats
}

/// Adjust a product's stock by `delta` (clamped at 0), persisting the change.
pub fn adjust_stock(id: i64, delta: i64) {
    edit_product(id, |p| p.stock = (p.stock + delta).max(0));
}

/// Persist the product form's editable fields (name/SKU/brand/category/description/
/// pricing/reorder level). Stock and images are managed by their own live actions, so
/// they're left untouched here. Empty name/SKU are ignored (they're required).
pub fn save_product(id: i64, e: ProductEdits) {
    edit_product(id, |p| {
        if !e.name.trim().is_empty() {
            p.name = e.name.trim().to_string();
        }
        if !e.sku.trim().is_empty() {
            p.sku = e.sku.trim().to_string();
        }
        p.brand = e.brand.trim().to_string();
        if !e.category.trim().is_empty() {
            p.category = e.category.trim().to_string();
        }
        p.description = e.description.trim().to_string();
        p.price_cents = e.price_cents.max(0);
        p.cost_cents = e.cost_cents.max(0);
        p.reorder_level = e.reorder_level.max(0);
    });
    toast("Product saved").show();
}

/// Append a gallery image URL to a product, persisting.
pub fn add_product_image(id: i64, url: String) {
    let url = url.trim().to_string();
    if url.is_empty() {
        return;
    }
    edit_product(id, |p| p.images.push(url.clone()));
}

/// Remove the gallery image at `index`, persisting.
pub fn remove_product_image(id: i64, index: usize) {
    edit_product(id, |p| {
        if index < p.images.len() {
            p.images.remove(index);
        }
    });
}

/// Promote the image at `index` to the cover (slot 0), persisting.
pub fn set_cover_image(id: i64, index: usize) {
    edit_product(id, |p| {
        if index < p.images.len() {
            p.images.swap(0, index);
        }
    });
}

pub fn delete_product(id: i64) {
    products_sig().update(|v| v.retain(|p| p.id != id));
    db::delete_product(id);
    toast("Product deleted").show();
}

/// Restock a low product to a comfortable level (reorder + a buffer).
pub fn reorder(id: i64) {
    edit_product(id, |p| p.stock = p.reorder_level + 24);
    toast("Reorder placed").duration(1.4).show();
}

fn edit_product(id: i64, f: impl FnOnce(&mut Product)) {
    let mut updated: Option<Product> = None;
    products_sig().update(|v| {
        if let Some(p) = v.iter_mut().find(|p| p.id == id) {
            f(p);
            updated = Some(p.clone());
        }
    });
    if let Some(p) = updated {
        db::upsert_product(&p);
    }
}

// ---------------------------------------------------------------------------
// Order actions
// ---------------------------------------------------------------------------

/// Advance (or set) an order's status, keeping its shipping timeline consistent.
pub fn set_order_status(id: i64, status: OrderStatus) {
    let mut updated: Option<Order> = None;
    orders_sig().update(|v| {
        if let Some(o) = v.iter_mut().find(|o| o.id == id) {
            o.status = status;
            sync_timeline(o);
            updated = Some(o.clone());
        }
    });
    if let Some(o) = updated {
        db::upsert_order(&o);
        push_notif(Notif {
            kind: NotifKind::Order,
            title: format!("Order {} · {}", o.code, status.label()),
            body: "Status updated".into(),
            time: "now".into(),
            read: false,
        });
    }
}

/// Mark each timeline step done/undone to match the order's status.
fn sync_timeline(o: &mut Order) {
    let through = match o.status {
        OrderStatus::Pending => 1,
        OrderStatus::Paid => 2,
        OrderStatus::Shipped => 4,
        OrderStatus::Delivered => 5,
        OrderStatus::Cancelled => 1,
    };
    for (i, ev) in o.shipping.iter_mut().enumerate() {
        ev.done = i < through;
        if ev.done && ev.date.is_empty() {
            ev.date = o.date.clone();
        }
    }
}

// ---------------------------------------------------------------------------
// Settings actions
// ---------------------------------------------------------------------------

/// Replace the whole settings object (from the Settings form), persisting.
pub fn save_settings(new: Settings) {
    settings_sig().set(new.clone());
    db::save_settings(&new);
    toast("Settings saved").show();
}

/// Flip a single boolean setting inline (the toggles), persisting immediately.
pub fn set_dark_mode(on: bool) {
    let mut s = settings();
    if s.dark_mode != on {
        s.dark_mode = on;
        toggle_theme();
        settings_sig().set(s.clone());
        db::save_settings(&s);
    }
}
pub fn set_flag(f: impl FnOnce(&mut Settings), _label: &str) {
    let mut s = settings();
    f(&mut s);
    settings_sig().set(s.clone());
    db::save_settings(&s);
}

// ---------------------------------------------------------------------------
// Notifications
// ---------------------------------------------------------------------------

pub fn mark_notifs_read() {
    notifs_sig().update(|v| v.iter_mut().for_each(|n| n.read = true));
}
fn push_notif(n: Notif) {
    notifs_sig().update(|v| v.insert(0, n));
}

// ---------------------------------------------------------------------------
// Cloud sync (optional; off the UI thread)
// ---------------------------------------------------------------------------

/// Pull fresh products from the cloud and merge them into the catalogue. No-op while a
/// sync is already running; reports success/failure via a toast + notification.
pub fn sync_from_cloud() {
    if syncing() {
        return;
    }
    syncing_sig().set(true);
    spawn(
        || net::fetch_products(30),
        |result| {
            syncing_sig().set(false);
            match result {
                Ok(list) => {
                    let mapped: Vec<Product> = list.iter().map(map_api_product).collect();
                    let n = mapped.len();
                    products_sig().update(|v| {
                        for p in &mapped {
                            match v.iter_mut().find(|x| x.id == p.id) {
                                Some(existing) => *existing = p.clone(),
                                None => v.push(p.clone()),
                            }
                        }
                    });
                    for p in &mapped {
                        db::upsert_product(p);
                    }
                    push_notif(Notif {
                        kind: NotifKind::Sync,
                        title: "Catalogue synced".into(),
                        body: format!("{n} products pulled from the cloud"),
                        time: "now".into(),
                        read: false,
                    });
                    toast(format!("Synced {n} products")).show();
                }
                Err(_) => {
                    toast("Sync failed — you're offline").variant(ToastVariant::Destructive).show();
                }
            }
        },
    );
}

fn map_api_product(p: &net::ApiProduct) -> Product {
    let images = if p.images.is_empty() {
        if p.thumbnail.is_empty() { Vec::new() } else { vec![p.thumbnail.clone()] }
    } else {
        p.images.clone()
    };
    let price_cents = (p.price * 100.0).round() as i64;
    Product {
        id: SYNC_BASE + p.id,
        sku: format!("SYNC-{:04}", p.id),
        name: if p.title.is_empty() { format!("Product {}", p.id) } else { p.title.clone() },
        category: capitalize(&p.category),
        brand: if p.brand.is_empty() { "Generic".into() } else { p.brand.clone() },
        price_cents,
        cost_cents: price_cents * 6 / 10,
        stock: p.stock.max(0),
        reorder_level: 12,
        rating: (p.rating * 10.0).round() / 10.0,
        description: p.description.clone(),
        images,
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => "Uncategorized".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Seed notifications
// ---------------------------------------------------------------------------

fn seed_notifs() -> Vec<Notif> {
    vec![
        Notif {
            kind: NotifKind::LowStock,
            title: "Low stock".into(),
            body: "Several products are at or below their reorder level.".into(),
            time: "2m".into(),
            read: false,
        },
        Notif {
            kind: NotifKind::Order,
            title: "New order #1057".into(),
            body: "Peak Outfitters placed an order.".into(),
            time: "1h".into(),
            read: false,
        },
        Notif {
            kind: NotifKind::Info,
            title: "Welcome to Northwind".into(),
            body: "Your data is saved locally and works fully offline.".into(),
            time: "1d".into(),
            read: true,
        },
    ]
}
