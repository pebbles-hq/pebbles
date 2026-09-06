//! The domain model for the inventory app — products, customers, orders, settings —
//! plus a deterministic **embedded seed** so the app is full of realistic data on the
//! very first run, entirely offline. Money is stored as integer cents everywhere.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Products
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Product {
    pub id: i64,
    pub sku: String,
    pub name: String,
    pub category: String,
    pub brand: String,
    pub price_cents: i64,
    pub cost_cents: i64,
    pub stock: i64,
    pub reorder_level: i64,
    pub rating: f64,
    pub description: String,
    /// Gallery image URLs (best-effort over the network; placeholders offline).
    pub images: Vec<String>,
}

/// A product's stock health, derived from `stock` vs `reorder_level`.
#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum StockStatus {
    InStock,
    LowStock,
    OutOfStock,
}

impl StockStatus {
    pub fn label(self) -> &'static str {
        match self {
            StockStatus::InStock => "In stock",
            StockStatus::LowStock => "Low stock",
            StockStatus::OutOfStock => "Out of stock",
        }
    }
}

impl Product {
    pub fn status(&self) -> StockStatus {
        if self.stock <= 0 {
            StockStatus::OutOfStock
        } else if self.stock <= self.reorder_level {
            StockStatus::LowStock
        } else {
            StockStatus::InStock
        }
    }
    /// Unit margin in cents (price − cost).
    pub fn margin_cents(&self) -> i64 {
        self.price_cents - self.cost_cents
    }
    /// Total value of stock on hand, at cost.
    pub fn stock_value_cents(&self) -> i64 {
        self.cost_cents * self.stock.max(0)
    }
    pub fn thumb(&self) -> Option<String> {
        self.images.first().cloned()
    }
}

// ---------------------------------------------------------------------------
// Customers
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Customer {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub phone: String,
    pub company: String,
    pub since: String,
}

// ---------------------------------------------------------------------------
// Orders
// ---------------------------------------------------------------------------

#[derive(Clone, Serialize, Deserialize)]
pub struct OrderLine {
    pub product_id: i64,
    pub name: String,
    pub qty: i64,
    pub unit_cents: i64,
}

impl OrderLine {
    pub fn line_total_cents(&self) -> i64 {
        self.unit_cents * self.qty
    }
}

/// One step in an order's fulfilment timeline.
#[derive(Clone, Serialize, Deserialize)]
pub struct ShipEvent {
    pub label: String,
    pub date: String,
    pub done: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Pending,
    Paid,
    Shipped,
    Delivered,
    Cancelled,
}

impl OrderStatus {
    pub fn label(self) -> &'static str {
        match self {
            OrderStatus::Pending => "Pending",
            OrderStatus::Paid => "Paid",
            OrderStatus::Shipped => "Shipped",
            OrderStatus::Delivered => "Delivered",
            OrderStatus::Cancelled => "Cancelled",
        }
    }
    /// Every status, for filter dropdowns.
    pub fn all() -> [OrderStatus; 5] {
        [
            OrderStatus::Pending,
            OrderStatus::Paid,
            OrderStatus::Shipped,
            OrderStatus::Delivered,
            OrderStatus::Cancelled,
        ]
    }
    // Parses the status back from its stored SQLite label — used only by the native DB
    // (`db::native`); the web build keeps typed `Order`s in memory, so it's dead there.
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    pub fn from_label(s: &str) -> OrderStatus {
        match s {
            "Paid" => OrderStatus::Paid,
            "Shipped" => OrderStatus::Shipped,
            "Delivered" => OrderStatus::Delivered,
            "Cancelled" => OrderStatus::Cancelled,
            _ => OrderStatus::Pending,
        }
    }
}

#[derive(Clone)]
pub struct Order {
    pub id: i64,
    pub code: String,
    pub customer_id: i64,
    pub date: String,
    pub status: OrderStatus,
    pub items: Vec<OrderLine>,
    pub shipping: Vec<ShipEvent>,
}

impl Order {
    pub fn subtotal_cents(&self) -> i64 {
        self.items.iter().map(OrderLine::line_total_cents).sum()
    }
    pub fn item_count(&self) -> i64 {
        self.items.iter().map(|l| l.qty).sum()
    }
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Settings {
    pub company: String,
    pub email: String,
    /// Index into [`CURRENCIES`].
    pub currency: usize,
    pub tax_rate: f64,
    pub low_stock_threshold: i64,
    pub dark_mode: bool,
    pub email_notifications: bool,
    pub weekly_report: bool,
    pub auto_reorder: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            company: "Northwind Supply Co.".into(),
            email: "ops@northwind.example".into(),
            currency: 0,
            tax_rate: 8.5,
            low_stock_threshold: 12,
            dark_mode: false,
            email_notifications: true,
            weekly_report: true,
            auto_reorder: false,
        }
    }
}

/// (symbol, code) pairs offered in Settings.
pub const CURRENCIES: [(&str, &str); 4] = [("$", "USD"), ("€", "EUR"), ("£", "GBP"), ("¥", "JPY")];

impl Settings {
    pub fn currency_symbol(&self) -> &'static str {
        CURRENCIES.get(self.currency).map(|c| c.0).unwrap_or("$")
    }
}

// ---------------------------------------------------------------------------
// Money formatting
// ---------------------------------------------------------------------------

/// Format integer cents as `1,234.56` (no symbol — the UI prepends the currency).
#[allow(clippy::manual_is_multiple_of)]
pub fn money(cents: i64) -> String {
    let neg = cents < 0;
    let cents = cents.unsigned_abs();
    let whole = cents / 100;
    let frac = cents % 100;
    // Group the whole part with thousands separators.
    let digits = whole.to_string();
    let mut grouped = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            grouped.push(',');
        }
        grouped.push(ch);
    }
    format!("{}{grouped}.{frac:02}", if neg { "-" } else { "" })
}

/// Format cents with a currency symbol, e.g. `$1,234.56`.
pub fn money_sym(cents: i64, symbol: &str) -> String {
    format!("{symbol}{}", money(cents))
}

// ---------------------------------------------------------------------------
// Embedded seed — deterministic, offline, realistic-enough
// ---------------------------------------------------------------------------

/// A picsum image URL for a product/gallery slot (best-effort; offline shows a
/// placeholder tile). Seeded so a product's images are stable across runs.
pub fn product_image(id: i64, n: i64) -> String {
    format!("https://picsum.photos/seed/prod-{id}-{n}/480/360")
}

const CATEGORIES: [&str; 6] = ["Laptops", "Phones", "Audio", "Displays", "Accessories", "Cameras"];
const BRANDS: [&str; 6] = ["Acme", "Nimbus", "Vertex", "Lumen", "Cobalt", "Aperture"];
const MODELS: [&str; 8] = ["Pro", "Air", "Max", "Mini", "Ultra", "Studio", "Edge", "Go"];

/// Build the embedded product catalogue (24 items across categories/brands).
pub fn seed_products() -> Vec<Product> {
    let mut out = Vec::new();
    for i in 0..24i64 {
        let cat = CATEGORIES[(i as usize) % CATEGORIES.len()];
        let brand = BRANDS[(i as usize / 2) % BRANDS.len()];
        let model = MODELS[(i as usize) % MODELS.len()];
        let id = i + 1;
        // Deterministic-but-varied numbers from the index.
        let base = 4900 + (i * 1737 % 90000);
        let price_cents = base + 99;
        let cost_cents = price_cents * 6 / 10;
        let stock = (i * 7 + 3) % 40; // 0..40 → some low / out of stock
        let reorder = 12;
        let rating = 3.6 + ((i * 3 % 14) as f64) / 10.0;
        let name = format!("{brand} {} {model}", singular(cat));
        out.push(Product {
            id,
            sku: format!("{}-{:04}", cat[..3].to_uppercase(), 1000 + id),
            name,
            category: cat.to_string(),
            brand: brand.to_string(),
            price_cents,
            cost_cents,
            stock,
            reorder_level: reorder,
            rating: (rating * 10.0).round() / 10.0,
            description: format!(
                "The {brand} {model} is a {} built for everyday reliability — a dependable pick in the \
                 {cat} lineup with a strong price-to-performance balance.",
                singular(cat).to_lowercase()
            ),
            images: vec![product_image(id, 1), product_image(id, 2), product_image(id, 3)],
        });
    }
    out
}

/// The singular noun for a category (for product names).
fn singular(cat: &str) -> &'static str {
    match cat {
        "Laptops" => "Laptop",
        "Phones" => "Phone",
        "Audio" => "Headset",
        "Displays" => "Display",
        "Accessories" => "Dock",
        "Cameras" => "Camera",
        _ => "Device",
    }
}

const CUSTOMER_NAMES: [(&str, &str); 10] = [
    ("Ava Bennett", "Skyline Retail"),
    ("Liam Carter", "Harbor Foods"),
    ("Noah Diaz", "Peak Outfitters"),
    ("Emma Foster", "Bright Labs"),
    ("Olivia Grant", "Cedar & Co."),
    ("Mason Hughes", "Ironwood Tools"),
    ("Sophia Iyer", "Lumen Studio"),
    ("Lucas Kim", "Northstar Freight"),
    ("Mia Lopez", "Vantage Media"),
    ("Ethan Moore", "Cobalt Systems"),
];

pub fn seed_customers() -> Vec<Customer> {
    CUSTOMER_NAMES
        .iter()
        .enumerate()
        .map(|(i, &(name, company))| {
            let handle = name.split_whitespace().next().unwrap_or("user").to_lowercase();
            Customer {
                id: i as i64 + 1,
                name: name.to_string(),
                email: format!(
                    "{handle}@{}.example",
                    company.split_whitespace().next().unwrap_or("co").to_lowercase()
                ),
                phone: format!("+1 (555) {:03}-{:04}", 200 + i * 7, 1000 + i * 137 % 9000),
                company: company.to_string(),
                since: format!("20{}-0{}-1{}", 22 + i % 3, 1 + i % 9, i % 9),
            }
        })
        .collect()
}

/// Synthesize a set of orders referencing the seeded products/customers.
pub fn seed_orders(products: &[Product], customers: &[Customer]) -> Vec<Order> {
    if products.is_empty() || customers.is_empty() {
        return Vec::new();
    }
    let statuses = OrderStatus::all();
    let mut out = Vec::new();
    for i in 0..16i64 {
        let customer = &customers[(i as usize) % customers.len()];
        let status = statuses[(i as usize) % statuses.len()];
        // 1–3 line items, picked deterministically.
        let n_items = 1 + (i % 3);
        let mut items = Vec::new();
        for k in 0..n_items {
            let p = &products[((i * 3 + k * 5) as usize) % products.len()];
            let qty = 1 + (i + k) % 4;
            items.push(OrderLine { product_id: p.id, name: p.name.clone(), qty, unit_cents: p.price_cents });
        }
        out.push(Order {
            id: i + 1,
            code: format!("#{:04}", 1042 + i),
            customer_id: customer.id,
            date: format!("2024-1{}-{:02}", i % 2, 1 + (i * 3) % 27),
            status,
            shipping: shipping_for(status, &format!("2024-1{}-{:02}", i % 2, 1 + (i * 3) % 27)),
            items,
        });
    }
    out
}

/// A fulfilment timeline whose completed steps reflect the order status.
fn shipping_for(status: OrderStatus, date: &str) -> Vec<ShipEvent> {
    let steps = ["Order placed", "Payment confirmed", "Packed", "Shipped", "Delivered"];
    let done_through = match status {
        OrderStatus::Pending => 1,
        OrderStatus::Paid => 2,
        OrderStatus::Shipped => 4,
        OrderStatus::Delivered => 5,
        OrderStatus::Cancelled => 1,
    };
    steps
        .iter()
        .enumerate()
        .map(|(i, &label)| ShipEvent {
            label: label.to_string(),
            date: if i < done_through { date.to_string() } else { String::new() },
            done: i < done_through,
        })
        .collect()
}
