//! [`canvas`] (H2) — a custom-painting widget. Give it a painter closure that draws
//! into a [`Canvas`] each paint; signals read **in the owning component** (not inside
//! the painter) drive re-renders, so reactivity is free. Unblocks charts + Gravel.
//!
//! ```ignore
//! canvas(move |c: &mut Canvas<'_>| {
//!     c.fill_rrect(Rect::new(0.0, 0.0, 80.0, 40.0), 8.0, theme().colors.primary);
//! })
//! .width(320.0)
//! .height(180.0) // unsized → fills the parent constraints
//! ```

use std::rc::Rc;

use pebbles_render::{Canvas, RenderCanvas, RenderObject};

use crate::widgets::SizedBox;
use pebbles_core::widget::{AnyWidget, IntoWidget, RenderWidget};

/// The leaf render-widget wrapping [`RenderCanvas`]. Not constructed directly — use
/// [`canvas`], which adds the `.width()/.height()` sizing sugar.
#[derive(Clone)]
struct CanvasLeaf {
    painter: Rc<dyn Fn(&mut Canvas<'_>)>,
}

pebbles_core::render_widget!(CanvasLeaf);

impl RenderWidget for CanvasLeaf {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderCanvas::new(self.painter.clone()))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderCanvas>() {
            // Swap the painter; the reconcile marks the node paint-dirty.
            r.painter = self.painter.clone();
        }
    }
}

/// A custom-painting widget. Build with [`canvas`]; size with `.width()/.height()`.
pub struct CanvasWidget {
    painter: Rc<dyn Fn(&mut Canvas<'_>)>,
    width: Option<f64>,
    height: Option<f64>,
}

/// Create a [`CanvasWidget`] that runs `painter` each paint.
pub fn canvas(painter: impl Fn(&mut Canvas<'_>) + 'static) -> CanvasWidget {
    CanvasWidget { painter: Rc::new(painter), width: None, height: None }
}

impl CanvasWidget {
    /// Fix the canvas width (unset → fills the parent's width).
    pub fn width(mut self, width: f64) -> Self {
        self.width = Some(width);
        self
    }
    /// Fix the canvas height (unset → fills the parent's height).
    pub fn height(mut self, height: f64) -> Self {
        self.height = Some(height);
        self
    }
}

impl IntoWidget for CanvasWidget {
    fn into_widget(self) -> AnyWidget {
        let leaf = CanvasLeaf { painter: self.painter };
        // A SizedBox applies the explicit dimensions (both None → the leaf fills).
        SizedBox::new(self.width, self.height, Some(leaf.into_widget())).into_widget()
    }
}
