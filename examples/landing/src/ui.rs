//! Shared visual building blocks for the storefront: the boutique palette, network
//! images with a graceful offline placeholder, translucent "glass" surfaces, gradient
//! scrims, and the capped section wrapper.

use pebbles::prelude::*;

/// Max content width; sections fill the window but their content centers within this.
pub const CAP: f64 = 1160.0;

// --- palette (a warm, editorial boutique look) ------------------------------
pub fn ink() -> Color {
    Color::new([0.055, 0.055, 0.065, 1.0])
}
pub fn paper() -> Color {
    Color::new([0.98, 0.972, 0.955, 1.0])
}
pub fn paper_dim() -> Color {
    Color::new([0.945, 0.933, 0.910, 1.0])
}
pub fn accent() -> Color {
    Color::new([0.79, 0.40, 0.30, 1.0])
}
pub fn ink_muted() -> Color {
    Color::new([0.38, 0.37, 0.36, 1.0])
}

pub fn white() -> Color {
    Color::new([1.0, 1.0, 1.0, 1.0])
}

pub fn with_alpha(c: Color, a: f32) -> Color {
    let [r, g, b, _] = c.components;
    Color::new([r, g, b, a])
}

// --- images -----------------------------------------------------------------

/// A network image that fills its box (`Cover`), with a tinted placeholder while it
/// loads or when offline.
pub fn image_fill(url: String) -> AnyWidget {
    ImageView::network(url)
        .fit(ImageFit::Cover)
        .placeholder(
            container()
                .color(paper_dim())
                .alignment(Alignment::CENTER)
                .child(icon(lucide::IMAGE).size(30.0).color(with_alpha(ink(), 0.18))),
        )
        .into_widget()
}

/// A vertical scrim (transparent → `to`) laid over an image so text stays legible.
pub fn scrim(from: Color, to: Color) -> AnyWidget {
    container()
        .decoration(BoxDecoration::new().gradient(Gradient::linear(
            Alignment::TOP_CENTER,
            Alignment::BOTTOM_CENTER,
            [from, to],
        )))
        .into_widget()
}

// --- surfaces ---------------------------------------------------------------

/// A translucent "frosted" panel — semi-transparent fill + hairline, for glass cards
/// and the nav over imagery.
pub fn glass(radius: f64, child: impl IntoWidget) -> Container {
    container()
        .decoration(
            BoxDecoration::new()
                .color(with_alpha(white(), 0.12))
                .border(Border::new(with_alpha(white(), 0.28), 1.0))
                .radius(BorderRadius::all(radius)),
        )
        .child(child)
}

/// A translucent, white-outlined pill button for use over imagery.
pub fn glass_button(label: &str) -> AnyWidget {
    pressable(glass(
        10.0,
        container()
            .padding(EdgeInsets::symmetric(22.0, 13.0))
            .child(text(label.to_string()).size(15.0).weight(600.0).color(white())),
    ))
    .radius(10.0)
    .into_widget()
}

/// A small uppercase eyebrow label in the accent color.
pub fn eyebrow(s: &str, color: Color) -> AnyWidget {
    text(s.to_uppercase()).size(12.0).weight(700.0).letter_spacing(1.6).color(color).into_widget()
}

/// A rounded tag pill (e.g. "New", "Best seller").
pub fn pill(label: &str, bg: Color, fg: Color) -> AnyWidget {
    container()
        .decoration(BoxDecoration::new().color(bg).radius(BorderRadius::all(999.0)))
        .padding(EdgeInsets::symmetric(10.0, 5.0))
        .child(text(label.to_string()).size(11.5).weight(600.0).color(fg))
        .into_widget()
}

/// A full-bleed section: `bg` fills the window width; the content centers within [`CAP`].
pub fn section(bg: Color, vpad: f64, child: impl IntoWidget) -> AnyWidget {
    container()
        .color(bg)
        .padding(EdgeInsets::symmetric(0.0, vpad))
        .child(center(container().width(CAP).padding(EdgeInsets::symmetric(28.0, 0.0)).child(child)))
        .into_widget()
}
