use pebbles::prelude::*;

use crate::ui::{doc, samples, screen};

/// Embedded at compile time (the repository's own sample images; no network).
const JPEG: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/pebbles-white.jpeg"));
const PNG_ALPHA: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/pebbles-transparent.png"));

pub fn images() -> Element {
    let img = || ImageView::memory(JPEG);
    let grey = || ImageView::asset(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/pebbles-grey.jpeg"));
    screen("Images").body(children![
        doc("Sources").body(samples(vec![
            ("memory bytes (JPEG)", img().size(200.0, 140.0).radius(BorderRadius::all(10.0)).into_widget()),
            (
                "file asset",
                ImageView::asset(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/pebbles-grey.jpeg"))
                    .size(200.0, 140.0)
                    .radius(BorderRadius::all(10.0))
                    .into_widget(),
            ),
            ("PNG with alpha", ImageView::memory(PNG_ALPHA).size(140.0, 140.0).into_widget()),
        ])),
        doc("Fit modes").body(samples(vec![
            ("cover (180×90)", grey().size(180.0, 90.0).fit(ImageFit::Cover).into_widget()),
            ("contain (180×90)", grey().size(180.0, 90.0).fit(ImageFit::Contain).into_widget()),
            ("fill (180×90)", grey().size(180.0, 90.0).fit(ImageFit::Fill).into_widget()),
            (
                "cover, circular",
                grey().size(90.0, 90.0).fit(ImageFit::Cover).radius(BorderRadius::all(45.0)).into_widget()
            ),
        ])),
        doc("Error").body(samples(vec![(
            "missing file → fallback",
            ImageView::asset("/nonexistent/missing.png")
                .size(200.0, 120.0)
                .radius(BorderRadius::all(10.0))
                .into_widget(),
        )])),
    ])
}
