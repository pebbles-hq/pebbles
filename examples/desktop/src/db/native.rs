//! **Local persistence (native)** — a real embedded SQLite database (bundled, no system
//! deps), living in the user's data dir. This is what makes the app stateful and fully
//! offline: the store loads everything from here on startup and writes every change
//! back.
//!
//! The connection lives on the UI thread (SQLite calls here are fast, local, and only
//! made from the reactive thread — including from background-fetch callbacks, which
//! `spawn` delivers back onto the UI thread). If the database can't be opened for any
//! reason, every function degrades to a no-op / empty result and the app runs purely
//! in memory — it never panics on a storage hiccup.
//!
//! This module is native-only; the web build uses the in-memory [`super::web`] twin.

use std::cell::RefCell;
use std::path::PathBuf;

use rusqlite::{Connection, params};

use crate::model::{Customer, Order, OrderLine, OrderStatus, Product, Settings, ShipEvent};

thread_local! {
    static DB: RefCell<Option<Connection>> = const { RefCell::new(None) };
}

/// Where the database file lives (`$XDG_DATA_HOME`/`~/.local/share`/cwd fallback).
fn db_path() -> PathBuf {
    let base = std::env::var("XDG_DATA_HOME")
        .ok()
        .or_else(|| std::env::var("HOME").ok().map(|h| format!("{h}/.local/share")))
        .unwrap_or_else(|| ".".to_string());
    let dir = PathBuf::from(base).join("pebbles-inventory");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("inventory.db")
}

/// Open the database and create the schema. Call once at startup. Safe to call again
/// (no-op once open).
pub fn init() {
    DB.with(|slot| {
        if slot.borrow().is_some() {
            return;
        }
        match Connection::open(db_path()).and_then(|c| {
            c.execute_batch(SCHEMA)?;
            Ok(c)
        }) {
            Ok(conn) => *slot.borrow_mut() = Some(conn),
            Err(e) => eprintln!("[db] running in-memory (open failed): {e}"),
        }
    });
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS products (
    id INTEGER PRIMARY KEY, sku TEXT, name TEXT, category TEXT, brand TEXT,
    price_cents INTEGER, cost_cents INTEGER, stock INTEGER, reorder_level INTEGER,
    rating REAL, description TEXT, images TEXT
);
CREATE TABLE IF NOT EXISTS customers (
    id INTEGER PRIMARY KEY, name TEXT, email TEXT, phone TEXT, company TEXT, since TEXT
);
CREATE TABLE IF NOT EXISTS orders (
    id INTEGER PRIMARY KEY, code TEXT, customer_id INTEGER, date TEXT, status TEXT,
    items TEXT, shipping TEXT
);
CREATE TABLE IF NOT EXISTS settings (
    id INTEGER PRIMARY KEY CHECK (id = 1), company TEXT, email TEXT, currency INTEGER,
    tax_rate REAL, low_stock_threshold INTEGER, dark_mode INTEGER,
    email_notifications INTEGER, weekly_report INTEGER, auto_reorder INTEGER
);
";

/// Run `f` with the open connection, swallowing any error (→ `None`).
fn with<T>(f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Option<T> {
    DB.with(|slot| slot.borrow().as_ref().and_then(|c| f(c).ok()))
}

/// Whether the catalogue is empty (→ first run, seed it).
pub fn is_empty() -> bool {
    with(|c| c.query_row("SELECT COUNT(*) FROM products", [], |r| r.get::<_, i64>(0)))
        .map(|n| n == 0)
        .unwrap_or(true)
}

// ---------------------------------------------------------------------------
// Products
// ---------------------------------------------------------------------------

pub fn load_products() -> Vec<Product> {
    with(|c| {
        let mut stmt = c.prepare(
            "SELECT id,sku,name,category,brand,price_cents,cost_cents,stock,reorder_level,rating,\
             description,images FROM products ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(Product {
                id: r.get(0)?,
                sku: r.get(1)?,
                name: r.get(2)?,
                category: r.get(3)?,
                brand: r.get(4)?,
                price_cents: r.get(5)?,
                cost_cents: r.get(6)?,
                stock: r.get(7)?,
                reorder_level: r.get(8)?,
                rating: r.get(9)?,
                description: r.get(10)?,
                images: serde_json::from_str(&r.get::<_, String>(11)?).unwrap_or_default(),
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect::<Vec<_>>())
    })
    .unwrap_or_default()
}

pub fn upsert_product(p: &Product) {
    let images = serde_json::to_string(&p.images).unwrap_or_else(|_| "[]".into());
    let _ = with(|c| {
        c.execute(
            "INSERT OR REPLACE INTO products \
             (id,sku,name,category,brand,price_cents,cost_cents,stock,reorder_level,rating,description,images) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![
                p.id, p.sku, p.name, p.category, p.brand, p.price_cents, p.cost_cents, p.stock,
                p.reorder_level, p.rating, p.description, images
            ],
        )
    });
}

pub fn delete_product(id: i64) {
    let _ = with(|c| c.execute("DELETE FROM products WHERE id = ?1", params![id]));
}

// ---------------------------------------------------------------------------
// Customers
// ---------------------------------------------------------------------------

pub fn load_customers() -> Vec<Customer> {
    with(|c| {
        let mut stmt = c.prepare("SELECT id,name,email,phone,company,since FROM customers ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok(Customer {
                id: r.get(0)?,
                name: r.get(1)?,
                email: r.get(2)?,
                phone: r.get(3)?,
                company: r.get(4)?,
                since: r.get(5)?,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect::<Vec<_>>())
    })
    .unwrap_or_default()
}

pub fn upsert_customer(c: &Customer) {
    let _ = with(|conn| {
        conn.execute(
            "INSERT OR REPLACE INTO customers (id,name,email,phone,company,since) \
             VALUES (?1,?2,?3,?4,?5,?6)",
            params![c.id, c.name, c.email, c.phone, c.company, c.since],
        )
    });
}

// ---------------------------------------------------------------------------
// Orders
// ---------------------------------------------------------------------------

pub fn load_orders() -> Vec<Order> {
    with(|c| {
        let mut stmt =
            c.prepare("SELECT id,code,customer_id,date,status,items,shipping FROM orders ORDER BY id DESC")?;
        let rows = stmt.query_map([], |r| {
            let items: Vec<OrderLine> = serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or_default();
            let shipping: Vec<ShipEvent> = serde_json::from_str(&r.get::<_, String>(6)?).unwrap_or_default();
            Ok(Order {
                id: r.get(0)?,
                code: r.get(1)?,
                customer_id: r.get(2)?,
                date: r.get(3)?,
                status: OrderStatus::from_label(&r.get::<_, String>(4)?),
                items,
                shipping,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect::<Vec<_>>())
    })
    .unwrap_or_default()
}

pub fn upsert_order(o: &Order) {
    let items = serde_json::to_string(&o.items).unwrap_or_else(|_| "[]".into());
    let shipping = serde_json::to_string(&o.shipping).unwrap_or_else(|_| "[]".into());
    let _ = with(|c| {
        c.execute(
            "INSERT OR REPLACE INTO orders (id,code,customer_id,date,status,items,shipping) \
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![o.id, o.code, o.customer_id, o.date, o.status.label(), items, shipping],
        )
    });
}

// ---------------------------------------------------------------------------
// Settings (single row, id = 1)
// ---------------------------------------------------------------------------

pub fn load_settings() -> Option<Settings> {
    with(|c| {
        c.query_row(
            "SELECT company,email,currency,tax_rate,low_stock_threshold,dark_mode,\
             email_notifications,weekly_report,auto_reorder FROM settings WHERE id = 1",
            [],
            |r| {
                Ok(Settings {
                    company: r.get(0)?,
                    email: r.get(1)?,
                    currency: r.get::<_, i64>(2)? as usize,
                    tax_rate: r.get(3)?,
                    low_stock_threshold: r.get(4)?,
                    dark_mode: r.get::<_, i64>(5)? != 0,
                    email_notifications: r.get::<_, i64>(6)? != 0,
                    weekly_report: r.get::<_, i64>(7)? != 0,
                    auto_reorder: r.get::<_, i64>(8)? != 0,
                })
            },
        )
    })
}

pub fn save_settings(s: &Settings) {
    let _ = with(|c| {
        c.execute(
            "INSERT OR REPLACE INTO settings \
             (id,company,email,currency,tax_rate,low_stock_threshold,dark_mode,email_notifications,weekly_report,auto_reorder) \
             VALUES (1,?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                s.company,
                s.email,
                s.currency as i64,
                s.tax_rate,
                s.low_stock_threshold,
                s.dark_mode as i64,
                s.email_notifications as i64,
                s.weekly_report as i64,
                s.auto_reorder as i64,
            ],
        )
    });
}
