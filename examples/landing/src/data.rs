//! Sample catalogue data + image URLs for the storefront. Images are pulled from
//! picsum.photos (deterministic per seed, so the page looks the same every run) and
//! degrade to a tinted placeholder tile when offline.

/// A picsum image URL, seeded so it's stable across runs.
pub fn img(seed: &str, w: u32, h: u32) -> String {
    format!("https://picsum.photos/seed/{seed}/{w}/{h}")
}

/// A storefront product.
pub struct Product {
    pub name: &'static str,
    pub category: &'static str,
    pub price: &'static str,
    pub seed: &'static str,
    pub tag: Option<&'static str>,
}

pub const PRODUCTS: [Product; 8] = [
    Product {
        name: "Aurelia Wool Coat",
        category: "Outerwear",
        price: "$248",
        seed: "peb-coat",
        tag: Some("New"),
    },
    Product { name: "Meridian Knit", category: "Sweaters", price: "$128", seed: "peb-knit", tag: None },
    Product {
        name: "Sablé Leather Tote",
        category: "Bags",
        price: "$310",
        seed: "peb-tote",
        tag: Some("Best seller"),
    },
    Product { name: "Halcyon Silk Dress", category: "Dresses", price: "$196", seed: "peb-dress", tag: None },
    Product { name: "Terra Suede Boots", category: "Footwear", price: "$225", seed: "peb-boots", tag: None },
    Product {
        name: "Nordic Trench",
        category: "Outerwear",
        price: "$268",
        seed: "peb-trench",
        tag: Some("New"),
    },
    Product {
        name: "Ecru Cashmere Scarf",
        category: "Accessories",
        price: "$92",
        seed: "peb-scarf",
        tag: None,
    },
    Product { name: "Atlas Denim", category: "Denim", price: "$138", seed: "peb-denim", tag: None },
];

/// The featured banners for the hero carousel: (eyebrow, headline, seed).
pub const FEATURES: [(&str, &str, &str); 3] = [
    ("Autumn / Winter '26", "Coats made to outlast the season", "peb-hero-1"),
    ("The Atelier Edit", "Quiet luxury, drawn on the GPU", "peb-hero-2"),
    ("New Arrivals", "Softer knits. Warmer tones.", "peb-hero-3"),
];

/// Shop-by-category tiles: (label, item count, seed).
pub const CATEGORIES: [(&str, &str, &str); 4] = [
    ("Women", "128 styles", "peb-cat-women"),
    ("Men", "96 styles", "peb-cat-men"),
    ("Accessories", "64 styles", "peb-cat-acc"),
    ("Footwear", "52 styles", "peb-cat-shoes"),
];
