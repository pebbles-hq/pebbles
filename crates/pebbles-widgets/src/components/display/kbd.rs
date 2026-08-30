//! [`Kbd`] — a keyboard-key chip, shadcn's `Kbd`. `kbd("⌘K")`.

use pebbles_foundation::EdgeInsets;
use pebbles_render::{Border, BorderRadius, BoxDecoration};

use crate::theme::theme;
use crate::widgets::{Container, text};
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// A keyboard-key chip. Build with [`kbd`].
pub struct Kbd {
    keys: String,
}

/// Create a [`Kbd`] chip, e.g. `kbd("⌘K")` or `kbd("Ctrl+C")`.
pub fn kbd(keys: impl Into<String>) -> Kbd {
    Kbd { keys: keys.into() }
}

impl IntoWidget for Kbd {
    fn into_widget(self) -> AnyWidget {
        let c = theme().colors;
        Container::new()
            .decoration(
                BoxDecoration::new()
                    .color(c.secondary)
                    .border(Border::new(c.border, 1.0))
                    .radius(BorderRadius::all(4.0)),
            )
            .padding(EdgeInsets::symmetric(6.0, 2.0))
            .child(text(self.keys).size(11.0).weight(500.0).color(c.muted_foreground))
            .into_widget()
    }
}
