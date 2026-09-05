//! Optional **cloud sync** — pulls fresh products from a public test API
//! ([dummyjson.com](https://dummyjson.com)) so the "Sync" button can top up the
//! catalogue with real data + images. Blocking, off the UI thread (called inside
//! `spawn`), exactly like the mobile sample. The app is fully usable without ever
//! calling this — it's enrichment, not a dependency.

use std::time::Duration;

use serde::Deserialize;

const BASE: &str = "https://dummyjson.com";

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).build()
}

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
pub fn fetch_products(limit: u32) -> Result<Vec<ApiProduct>, String> {
    let url = format!(
        "{BASE}/products?limit={limit}&select=title,description,category,brand,price,rating,stock,thumbnail,images"
    );
    let body =
        agent().get(&url).call().map_err(|e| e.to_string())?.into_string().map_err(|e| e.to_string())?;
    serde_json::from_str::<ProductPage>(&body).map(|p| p.products).map_err(|e| e.to_string())
}
