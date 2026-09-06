//! [`RenderCanvas`] (H2) — a leaf that hands its painter a [`Canvas`]: an immediate-mode
//! drawing surface over the vello scene in the widget's local coordinates. Text stays
//! OUT of v1 (it needs a `TextEnv` at paint) — layer `Text` widgets above instead.

use std::rc::Rc;

use kurbo::{Affine, BezPath, Circle, Line, Point, Rect as KRect, RoundedRect, Stroke};
use pebbles_foundation::{Color, Offset, Rect, Size};
use peniko::{Brush, Fill};

use crate::constraints::BoxConstraints;
use crate::decoration::BlendMode;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// An immediate-mode drawing surface, in the canvas widget's **local** coordinates
/// (`(0,0)` = the widget's top-left). Every shape is translated to the widget's paint
/// origin for you. Keep painters allocation-light — they run on every paint.
pub struct Canvas<'a> {
    scene: crate::paint::Painter<'a>,
    origin: Offset,
    size: Size,
}

impl Canvas<'_> {
    /// The canvas size in logical pixels (local coordinate bounds).
    pub fn size(&self) -> Size {
        self.size
    }

    /// Local→window transform applied to every draw.
    fn xf(&self) -> Affine {
        Affine::translate((self.origin.x, self.origin.y))
    }

    /// Fill an axis-aligned rectangle.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        let r = KRect::new(rect.x0, rect.y0, rect.x1, rect.y1);
        self.scene.fill(Fill::NonZero, self.xf(), &Brush::Solid(color), None, &r);
    }

    /// Fill a rounded rectangle.
    pub fn fill_rrect(&mut self, rect: Rect, radius: f64, color: Color) {
        let r = RoundedRect::new(rect.x0, rect.y0, rect.x1, rect.y1, radius);
        self.scene.fill(Fill::NonZero, self.xf(), &Brush::Solid(color), None, &r);
    }

    /// Fill a circle.
    pub fn fill_circle(&mut self, center: Offset, radius: f64, color: Color) {
        let c = Circle::new(Point::new(center.x, center.y), radius);
        self.scene.fill(Fill::NonZero, self.xf(), &Brush::Solid(color), None, &c);
    }

    /// Stroke a straight line.
    pub fn stroke_line(&mut self, a: Offset, b: Offset, width: f64, color: Color) {
        let l = Line::new(Point::new(a.x, a.y), Point::new(b.x, b.y));
        self.scene.stroke(&Stroke::new(width), self.xf(), &Brush::Solid(color), None, &l);
    }

    /// Stroke an arbitrary kurbo path (build one with `BezPath`).
    pub fn stroke_path(&mut self, path: &BezPath, width: f64, color: Color) {
        self.scene.stroke(&Stroke::new(width), self.xf(), &Brush::Solid(color), None, path);
    }

    /// Clip subsequent draws to `rect` until the matching [`pop_clip`](Canvas::pop_clip).
    pub fn push_clip(&mut self, rect: Rect) {
        let r = KRect::new(rect.x0, rect.y0, rect.x1, rect.y1);
        self.scene.push_layer(Fill::NonZero, BlendMode::Normal, 1.0, self.xf(), &r);
    }

    /// Pop the most recent [`push_clip`](Canvas::push_clip).
    pub fn pop_clip(&mut self) {
        self.scene.pop_layer();
    }
}

/// A leaf render object that runs a painter closure each paint. Sizes to the biggest
/// bounded constraint (wrap in a sized box to give it explicit dimensions).
pub struct RenderCanvas {
    pub painter: Rc<dyn Fn(&mut Canvas<'_>)>,
}

impl RenderCanvas {
    pub fn new(painter: Rc<dyn Fn(&mut Canvas<'_>)>) -> Self {
        RenderCanvas { painter }
    }
}

impl RenderObject for RenderCanvas {
    fn layout(&mut self, _cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        let big = constraints.biggest();
        let w = if big.width.is_finite() { big.width } else { 0.0 };
        let h = if big.height.is_finite() { big.height } else { 0.0 };
        constraints.constrain(Size::new(w, h))
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let size = cx.size();
        let mut canvas = Canvas { scene: cx.scene.reborrow(), origin: offset, size };
        (self.painter)(&mut canvas);
    }

    fn debug_name(&self) -> &'static str {
        "RenderCanvas"
    }
}
