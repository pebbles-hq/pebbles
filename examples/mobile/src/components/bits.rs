//! Small shared building blocks reused across screens.

use pebbles::prelude::*;

/// A circular network avatar with a muted placeholder while it loads.
pub fn avatar(url: &str, size: f64) -> impl IntoWidget {
    let c = theme().colors;
    ImageView::network(url.to_string())
        .size(size, size)
        .radius(BorderRadius::all(size / 2.0))
        .fit(ImageFit::Cover)
        .placeholder(
            container()
                .decoration(BoxDecoration::new().color(c.secondary).shape(BoxShape::Circle))
                .width(size)
                .height(size),
        )
}

/// A pill button used for follow / edit actions.
pub fn pill(label: &str, filled: bool, on_tap: impl Fn() + 'static) -> impl IntoWidget {
    let c = theme().colors;
    let (bg, fg) = if filled { (c.primary, c.primary_foreground) } else { (c.secondary, c.foreground) };
    pressable(
        container()
            .decoration(BoxDecoration::new().color(bg).radius(BorderRadius::all(999.0)))
            .padding(EdgeInsets::symmetric(16.0, 7.0))
            .child(text(label.to_string()).size(13.0).weight(600.0).color(fg)),
    )
    .radius(999.0)
    .on_tap(on_tap)
}
