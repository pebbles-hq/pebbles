//! [`placeholder`] — a bordered box with a diagonal cross (Flutter's `Placeholder`):
//! the "not built yet" marker you drop in while roughing out a layout. Fills its space
//! unless given a fixed `.size(..)`.

use pebbles_foundation::{Color, Offset};

use crate::theme::theme;
use crate::widgets::{CanvasWidget, canvas};
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// A placeholder box. Built by [`placeholder`].
#[derive(Clone)]
pub struct Placeholder {
    color: Option<Color>,
    stroke: f64,
    width: Option<f64>,
    height: Option<f64>,
}

/// A placeholder box (border + an X). Fills its parent; give it a fixed [`size`](Placeholder::size)
/// when unconstrained.
pub fn placeholder() -> Placeholder {
    Placeholder { color: None, stroke: 2.0, width: None, height: None }
}

impl Placeholder {
    /// The line color (default: the theme's muted foreground).
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    /// The line width (default `2`).
    pub fn stroke_width(mut self, w: f64) -> Self {
        self.stroke = w.max(0.5);
        self
    }
    /// A fixed size (otherwise it fills the parent).
    pub fn size(mut self, w: f64, h: f64) -> Self {
        self.width = Some(w);
        self.height = Some(h);
        self
    }
}

impl IntoWidget for Placeholder {
    fn into_widget(self) -> AnyWidget {
        let color = self.color.unwrap_or_else(|| theme().colors.muted_foreground);
        let stroke = self.stroke;
        let mut cv: CanvasWidget = canvas(move |c| {
            let s = c.size();
            let (w, h) = (s.width, s.height);
            let (tl, tr, bl, br) =
                (Offset::new(0.0, 0.0), Offset::new(w, 0.0), Offset::new(0.0, h), Offset::new(w, h));
            // Border.
            c.stroke_line(tl, tr, stroke, color);
            c.stroke_line(tr, br, stroke, color);
            c.stroke_line(br, bl, stroke, color);
            c.stroke_line(bl, tl, stroke, color);
            // Diagonals.
            c.stroke_line(tl, br, stroke, color);
            c.stroke_line(tr, bl, stroke, color);
        });
        if let (Some(w), Some(h)) = (self.width, self.height) {
            cv = cv.width(w).height(h);
        }
        cv.into_widget()
    }
}
