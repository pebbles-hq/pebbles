//! [`RenderSpinner`] — a leaf render object that paints a circular loading
//! spinner: a 270° arc rotated by `angle`. The widget layer drives `angle` from a
//! looping animation (`create_loop`), so the arc spins.

use kurbo::{Affine, Arc, Cap, Point, Shape, Stroke, Vec2};
use pebbles_foundation::{Color, Offset, Size};

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// A spinning arc.
pub struct RenderSpinner {
    /// Rotation in radians (driven by the widget layer).
    pub angle: f64,
    pub color: Color,
    pub diameter: f64,
    pub stroke_width: f64,
}

impl RenderSpinner {
    pub fn new(diameter: f64, color: Color) -> Self {
        RenderSpinner { angle: 0.0, color, diameter, stroke_width: (diameter / 8.0).max(1.5) }
    }
}

impl RenderObject for RenderSpinner {
    fn layout(&mut self, _cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        constraints.constrain(Size::new(self.diameter, self.diameter))
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let r = (self.diameter - self.stroke_width) / 2.0;
        let center = Point::new(offset.x + self.diameter / 2.0, offset.y + self.diameter / 2.0);
        // A 270° sweep (3π/2) leaves a gap so the rotation reads as motion.
        let arc = Arc::new(center, Vec2::new(r, r), self.angle, std::f64::consts::FRAC_PI_2 * 3.0, 0.0);
        let stroke = Stroke::new(self.stroke_width).with_caps(Cap::Round);
        cx.scene.stroke(&stroke, Affine::IDENTITY, self.color, None, &arc.to_path(0.2));
    }

    fn debug_name(&self) -> &'static str {
        "RenderSpinner"
    }
}
