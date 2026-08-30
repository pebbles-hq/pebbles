//! [`ScrollArea`] — a bounded, scrollable region with a slim always-on scrollbar
//! (shadcn's `ScrollArea`). `scroll_area(content).height(240.0)`.

use crate::widgets::{Container, SingleChildScrollView};
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// A bounded scroll region. Build with [`scroll_area`]; set at least a height.
pub struct ScrollArea {
    child: AnyWidget,
    width: Option<f64>,
    height: Option<f64>,
    thickness: f64,
}

/// Create a [`ScrollArea`] around `content`.
pub fn scroll_area(content: impl IntoWidget) -> ScrollArea {
    ScrollArea { child: content.into_widget(), width: None, height: None, thickness: 6.0 }
}

impl ScrollArea {
    pub fn width(mut self, width: f64) -> Self {
        self.width = Some(width);
        self
    }
    pub fn height(mut self, height: f64) -> Self {
        self.height = Some(height);
        self
    }
    /// Scrollbar thickness (default `6`).
    pub fn thickness(mut self, thickness: f64) -> Self {
        self.thickness = thickness;
        self
    }
}

impl IntoWidget for ScrollArea {
    fn into_widget(self) -> AnyWidget {
        let view = SingleChildScrollView::vertical(self.child)
            .scrollbar_thickness(self.thickness)
            .always_scrollbar();
        let mut container = Container::new().child(view);
        if let Some(w) = self.width {
            container = container.width(w);
        }
        if let Some(h) = self.height {
            container = container.height(h);
        }
        container.into_widget()
    }
}
