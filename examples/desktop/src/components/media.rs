//! Media atoms: a star rating and a product thumbnail.

use pebbles::prelude::*;

use crate::model::Product;

pub fn stars(rating: f64) -> impl IntoWidget {
    let c = theme().colors;
    row(children![
        text("★".to_string()).size(13.0).color(palette::amber::S500),
        gap_w(3.0),
        text(format!("{rating:.1}")).size(12.5).color(c.muted_foreground),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .main_axis_size(MainAxisSize::Min)
}

/// A rounded product thumbnail — its first image, with a colored placeholder tile
/// (also the offline fallback).
pub fn thumb(p: &Product, size: f64) -> AnyWidget {
    let c = theme().colors;
    let placeholder = container()
        .decoration(BoxDecoration::new().color(c.secondary).radius(BorderRadius::all(10.0)))
        .width(size)
        .height(size)
        .alignment(Alignment::CENTER)
        .child(icon(lucide::PACKAGE).size(size * 0.42).color(c.muted_foreground));

    match p.thumb() {
        Some(url) => container()
            .decoration(BoxDecoration::new().color(c.secondary).radius(BorderRadius::all(10.0)))
            .clip()
            .width(size)
            .height(size)
            .child(ImageView::network(url).fit(ImageFit::Cover).placeholder(placeholder))
            .into_widget(),
        None => placeholder.into_widget(),
    }
}
