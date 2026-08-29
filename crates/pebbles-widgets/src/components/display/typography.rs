//! Typography helpers — themed [`Text`](crate::Text) presets: headings, body,
//! labels and muted text.

use crate::theme::theme;
use crate::widgets::{Text, text};

/// A large page/section heading.
pub fn heading(value: impl Into<String>) -> Text {
    let th = theme();
    text(value).size(30.0).bold().color(th.colors.foreground).line_height(1.2)
}

/// A card/subsection title.
pub fn title(value: impl Into<String>) -> Text {
    let th = theme();
    text(value).size(18.0).semibold().color(th.colors.foreground)
}

/// A supporting subtitle.
pub fn subtitle(value: impl Into<String>) -> Text {
    let th = theme();
    text(value).size(14.0).color(th.colors.muted_foreground)
}

/// Body text.
pub fn body(value: impl Into<String>) -> Text {
    let th = theme();
    text(value).size(th.font_size).color(th.colors.foreground).line_height(1.4)
}

/// A form/control label.
pub fn label(value: impl Into<String>) -> Text {
    let th = theme();
    text(value).size(13.0).weight(500.0).color(th.colors.foreground)
}

/// Muted secondary text.
pub fn muted(value: impl Into<String>) -> Text {
    let th = theme();
    text(value).size(13.0).color(th.colors.muted_foreground)
}
