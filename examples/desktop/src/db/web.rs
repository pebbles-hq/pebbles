//! **Local persistence (web)** — the in-memory twin of [`super::native`] for the wasm
//! build, where the bundled SQLite engine can't run. It exposes the exact same public
//! API, so `store.rs` is unchanged; the only behavioural difference is that data lives
//! in memory for the session (it isn't persisted across a page reload).

use std::cell::RefCell;

use crate::model::{Customer, Order, Product, Settings};

#[derive(Default)]
struct Store {
    products: Vec<Product>,
    customers: Vec<Customer>,
    orders: Vec<Order>,
    settings: Option<Settings>,
}

thread_local! {
    static DB: RefCell<Store> = RefCell::new(Store::default());
}

fn with<T>(f: impl FnOnce(&mut Store) -> T) -> T {
    DB.with(|d| f(&mut d.borrow_mut()))
}

/// Insert-or-replace by `id`, preserving insertion order for new rows.
fn upsert<T: Clone>(v: &mut Vec<T>, item: &T, id: impl Fn(&T) -> i64) {
    let target = id(item);
    match v.iter_mut().find(|x| id(x) == target) {
        Some(slot) => *slot = item.clone(),
        None => v.push(item.clone()),
    }
}

pub fn init() {}

pub fn is_empty() -> bool {
    with(|s| s.products.is_empty())
}

pub fn load_products() -> Vec<Product> {
    with(|s| {
        let mut v = s.products.clone();
        v.sort_by_key(|p| p.id);
        v
    })
}

pub fn upsert_product(p: &Product) {
    with(|s| upsert(&mut s.products, p, |x| x.id));
}

pub fn delete_product(id: i64) {
    with(|s| s.products.retain(|p| p.id != id));
}

pub fn load_customers() -> Vec<Customer> {
    with(|s| {
        let mut v = s.customers.clone();
        v.sort_by_key(|c| c.id);
        v
    })
}

pub fn upsert_customer(c: &Customer) {
    with(|s| upsert(&mut s.customers, c, |x| x.id));
}

pub fn load_orders() -> Vec<Order> {
    with(|s| {
        let mut v = s.orders.clone();
        v.sort_by(|a, b| b.id.cmp(&a.id)); // newest first, matching the SQL `ORDER BY id DESC`
        v
    })
}

pub fn upsert_order(o: &Order) {
    with(|s| upsert(&mut s.orders, o, |x| x.id));
}

pub fn load_settings() -> Option<Settings> {
    with(|s| s.settings.clone())
}

pub fn save_settings(v: &Settings) {
    with(|s| s.settings = Some(v.clone()));
}
