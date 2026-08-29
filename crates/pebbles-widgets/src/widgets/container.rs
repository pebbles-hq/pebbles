//! [`Container`] — the Flutter convenience composite. A single widget that, via a
//! stack of simpler render widgets, applies a decoration (color, border, radius,
//! shadow), padding, sizing and alignment. A [`StatelessWidget`](pebbles_core::StatelessWidget):
//! it owns no render object of its own, it just composes.

use pebbles_foundation::{Alignment, Color, EdgeInsets};
use pebbles_render::{Border, BorderRadius, BoxDecoration, BoxShadow};

use pebbles_core::widget::{AnyWidget, IntoWidget};
use crate::widgets::{Align, DecoratedBox, Padding, SizedBox};

/// A convenience box combining decoration, padding, sizing and alignment.
#[derive(Clone, Default)]
pub struct Container {
    decoration: Option<BoxDecoration>,
    padding: Option<EdgeInsets>,
    width: Option<f64>,
    height: Option<f64>,
    alignment: Option<Alignment>,
    child: Option<AnyWidget>,
}

impl Container {
    /// An empty container. Chain the fluent setters to configure it.
    pub fn new() -> Self {
        Self::default()
    }

    fn deco(&mut self) -> &mut BoxDecoration {
        self.decoration.get_or_insert_with(BoxDecoration::new)
    }

    pub fn child(mut self, child: impl IntoWidget) -> Self {
        self.child = Some(child.into_widget());
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.deco().color = Some(color);
        self
    }
    pub fn radius(mut self, radius: BorderRadius) -> Self {
        self.deco().radius = radius;
        self
    }
    pub fn border(mut self, border: Border) -> Self {
        self.deco().border = Some(border);
        self
    }
    pub fn shadow(mut self, shadow: BoxShadow) -> Self {
        self.deco().shadows.push(shadow);
        self
    }
    /// Replace the whole decoration at once.
    pub fn decoration(mut self, decoration: BoxDecoration) -> Self {
        self.decoration = Some(decoration);
        self
    }
    pub fn padding(mut self, insets: EdgeInsets) -> Self {
        self.padding = Some(insets);
        self
    }
    pub fn width(mut self, width: f64) -> Self {
        self.width = Some(width);
        self
    }
    pub fn height(mut self, height: f64) -> Self {
        self.height = Some(height);
        self
    }
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = Some(alignment);
        self
    }
}

impl IntoWidget for Container {
    fn into_widget(mut self) -> AnyWidget {
        // Compose inner-to-outer exactly like Flutter's Container:
        //   child -> align -> padding -> decoration -> constraints(size)
        // `alignment` positions the child *inside* the (padded, sized) box; putting
        // it outermost — as a previous version did — instead moved the whole box and
        // pinned the child top-left (which broke e.g. the Switch thumb).
        let mut current: AnyWidget =
            self.child.take().unwrap_or_else(|| SizedBox::spacer(0.0, 0.0).into_widget());

        if let Some(alignment) = self.alignment {
            current = Align::new(alignment, current).into_widget();
        }
        if let Some(insets) = self.padding {
            current = Padding::new(insets, current).into_widget();
        }
        if let Some(decoration) = self.decoration.take() {
            current = DecoratedBox::new(decoration, current).into_widget();
        }
        if self.width.is_some() || self.height.is_some() {
            current = SizedBox::new(self.width, self.height, Some(current)).into_widget();
        }
        current
    }
}
