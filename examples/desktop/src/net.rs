//! Optional **cloud sync** — pulls fresh products from a public test API
//! ([dummyjson.com](https://dummyjson.com)) so the "Sync" button can top up the
//! catalogue with real data + images. Blocking, off the UI thread (called inside
//! `spawn`), exactly like the mobile sample. The app is fully usable without ever
//! calling this — it's enrichment, not a dependency.

//! Cloud sync is native-only (blocking HTTP via `ureq`, which doesn't build/run on
//! wasm); on the web build `fetch_products` returns an error so the "Sync" button
//! degrades to a friendly "offline" toast, exactly as it does when a native fetch fails.

use serde::Deserialize;

#[cfg(not(target_family = "wasm"))]
const BASE: &str = "https://dummyjson.com";

#[cfg(not(target_family = "wasm"))]
fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(std::time::Duration::from_secs(15)).build()
}

#[cfg(not(target_family = "wasm"))]
#[derive(Deserialize)]
struct ProductPage {
    products: Vec<ApiProduct>,
}

/// A product as the API returns it (only the fields we map).
#[derive(Deserialize)]
pub struct ApiProduct {
    pub id: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub brand: String,
    #[serde(default)]
    pub price: f64,
    #[serde(default)]
    pub rating: f64,
    #[serde(default)]
    pub stock: i64,
    #[serde(default)]
    pub thumbnail: String,
    #[serde(default)]
    pub images: Vec<String>,
}

/// Fetch up to `limit` products from the cloud. Blocking — call inside `spawn`.
#[cfg(not(target_family = "wasm"))]
pub fn fetch_products(limit: u32) -> Result<Vec<ApiProduct>, String> {
    let url = format!(
        "{BASE}/products?limit={limit}&select=title,description,category,brand,price,rating,stock,thumbnail,images"
    );
    let body =
        agent().get(&url).call().map_err(|e| e.to_string())?.into_string().map_err(|e| e.to_string())?;
    serde_json::from_str::<ProductPage>(&body).map(|p| p.products).map_err(|e| e.to_string())
}

/// Web build: no blocking HTTP client, so cloud sync is unavailable — the caller shows
/// the same "offline" toast it uses for a failed native fetch.
#[cfg(target_family = "wasm")]
pub fn fetch_products(_limit: u32) -> Result<Vec<ApiProduct>, String> {
    Err("cloud sync isn't available on the web build".into())
}
