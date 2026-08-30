//! [`Transform`] — rotate, scale, translate or skew a child (paint + hit-test),
//! around a configurable origin. Flutter's `Transform`.

use pebbles_foundation::Alignment;
use pebbles_render::{Affine, RenderObject, RenderTransform};

use pebbles_core::widget::{AnyWidget, IntoWidget, RenderWidget};

/// Applies an affine [`transform`] to its child.
#[derive(Clone)]
pub struct Transform {
    matrix: Affine,
    alignment: Alignment,
    child: Option<AnyWidget>,
}

/// Transform `child` by an arbitrary affine matrix (origin defaults to the center).
pub fn transform(matrix: Affine, child: impl IntoWidget) -> Transform {
    Transform { matrix, alignment: Alignment::CENTER, child: Some(child.into_widget()) }
}

impl Transform {
    /// Rotate `radians` clockwise around the origin.
    pub fn rotate(radians: f64, child: impl IntoWidget) -> Self {
        transform(Affine::rotate(radians), child)
    }
    /// Uniformly scale by `factor`.
    pub fn scale(factor: f64, child: impl IntoWidget) -> Self {
        transform(Affine::scale(factor), child)
    }
    /// Scale by `sx`/`sy` independently.
    pub fn scale_xy(sx: f64, sy: f64, child: impl IntoWidget) -> Self {
        transform(Affine::scale_non_uniform(sx, sy), child)
    }
    /// Translate by `dx`/`dy` at paint time (does not affect layout).
    pub fn translate(dx: f64, dy: f64, child: impl IntoWidget) -> Self {
        transform(Affine::translate((dx, dy)), child)
    }
    /// The origin the transform pivots around (`-1..1` per axis; default center).
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }
}

pebbles_core::render_widget!(Transform);

impl RenderWidget for Transform {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderTransform::new(self.matrix, self.alignment))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(t) = object.downcast_mut::<RenderTransform>() {
            t.matrix = self.matrix;
            t.alignment = self.alignment;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}
