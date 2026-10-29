//! [`RenderCanvas`] (H2) — a leaf that hands its painter a [`Canvas`]: an immediate-mode
//! drawing surface over the scene in the widget's local coordinates — shapes, gradients,
//! dashes, clips, and text laid out exactly like the `Text` widget.

use std::rc::Rc;

use kurbo::{Affine, BezPath, Cap, Circle, Line, Point, Rect as KRect, RoundedRect, Shape, Stroke};
use pebbles_foundation::{Color, Offset, Rect, Size};
use peniko::{Brush, Fill};

use crate::constraints::BoxConstraints;
use crate::decoration::{BlendMode, Gradient};
use crate::object::RenderObject;
use crate::objects::decorated::gradient_brush;
use crate::objects::{ParagraphStyle, TextSpanStyle};
use crate::text::paint::TextPaint;
use crate::tree::{LayoutCx, PaintCx};
use smallvec::SmallVec;

/// A `width` stroke with butt caps (the Skia / Flutter / CSS default).
fn butt(width: f64) -> Stroke {
    Stroke::new(width).with_caps(Cap::Butt)
}

/// An immediate-mode drawing surface, in the canvas widget's **local** coordinates
/// (`(0,0)` = the widget's top-left). Every shape is translated to the widget's paint
/// origin for you. Keep painters allocation-light — they run on every paint.
pub struct Canvas<'a> {
    scene: crate::paint::Painter<'a>,
    text: &'a mut crate::text::TextEnv,
    origin: Offset,
    size: Size,
    /// The window-space rect that can show (culls off-screen text lines).
    visible: Rect,
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

    /// Stroke a straight line. Caps are butt — the line ends exactly at `a` and `b`, as
    /// Skia / Flutter `Paint` / CSS `stroke-linecap` default to (kurbo's default is round).
    pub fn stroke_line(&mut self, a: Offset, b: Offset, width: f64, color: Color) {
        let l = Line::new(Point::new(a.x, a.y), Point::new(b.x, b.y));
        self.scene.stroke(&butt(width), self.xf(), &Brush::Solid(color), None, &l);
    }

    /// Stroke an arbitrary kurbo path, butt-capped (build one with `BezPath`).
    pub fn stroke_path(&mut self, path: &BezPath, width: f64, color: Color) {
        self.scene.stroke(&butt(width), self.xf(), &Brush::Solid(color), None, path);
    }

    /// Fill an arbitrary kurbo path (non-zero winding) — the basis for area fills, pie
    /// wedges, and any custom filled shape. Build one with `BezPath`.
    pub fn fill_path(&mut self, path: &BezPath, color: Color) {
        self.scene.fill(Fill::NonZero, self.xf(), &Brush::Solid(color), None, path);
    }

    // ---- gradient fills -----------------------------------------------------
    // The [`Gradient`] endpoints are `Alignment`s within the shape's bounds (the same
    // spec `BoxDecoration` uses), so a gradient scales with any size. Linear, radial,
    // and sweep are all supported.

    /// Fill an axis-aligned rectangle with a gradient positioned within it.
    pub fn fill_rect_gradient(&mut self, rect: Rect, gradient: &Gradient) {
        let r = KRect::new(rect.x0, rect.y0, rect.x1, rect.y1);
        let brush = Brush::Gradient(gradient_brush(gradient, rect));
        self.scene.fill(Fill::NonZero, self.xf(), &brush, None, &r);
    }

    /// Fill a rounded rectangle with a gradient positioned within it.
    pub fn fill_rrect_gradient(&mut self, rect: Rect, radius: f64, gradient: &Gradient) {
        let r = RoundedRect::new(rect.x0, rect.y0, rect.x1, rect.y1, radius);
        let brush = Brush::Gradient(gradient_brush(gradient, rect));
        self.scene.fill(Fill::NonZero, self.xf(), &brush, None, &r);
    }

    /// Fill an arbitrary kurbo path with a gradient positioned within the path's
    /// bounding box (non-zero winding).
    pub fn fill_path_gradient(&mut self, path: &BezPath, gradient: &Gradient) {
        let bounds = path.bounding_box();
        let brush = Brush::Gradient(gradient_brush(gradient, bounds));
        self.scene.fill(Fill::NonZero, self.xf(), &brush, None, path);
    }

    // ---- dashed strokes -----------------------------------------------------
    // `dashes` is an on/off length pattern (e.g. `&[6.0, 4.0]` = 6px dash, 4px gap),
    // repeated along the path; `dash_offset` shifts the start into the pattern.

    /// Stroke a straight line with a dash pattern.
    pub fn stroke_line_dashed(
        &mut self,
        a: Offset,
        b: Offset,
        width: f64,
        color: Color,
        dashes: &[f64],
        dash_offset: f64,
    ) {
        let l = Line::new(Point::new(a.x, a.y), Point::new(b.x, b.y));
        let stroke = butt(width).with_dashes(dash_offset, dashes);
        self.scene.stroke(&stroke, self.xf(), &Brush::Solid(color), None, &l);
    }

    /// Stroke an arbitrary kurbo path with a dash pattern.
    pub fn stroke_path_dashed(
        &mut self,
        path: &BezPath,
        width: f64,
        color: Color,
        dashes: &[f64],
        dash_offset: f64,
    ) {
        let stroke = butt(width).with_dashes(dash_offset, dashes);
        self.scene.stroke(&stroke, self.xf(), &Brush::Solid(color), None, path);
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

    // ---- text ---------------------------------------------------------------
    // Laid out by the same pipeline as the `Text` widget (style resolution, fonts, line
    // breaking, decorations), through the window's text caches — so a canvas-drawn row
    // lines up with a `Text` row in the same style.

    /// Draw `text` in `style` as ONE unbounded line with its top-left at `at` (local).
    /// Returns the laid-out size. Flutter: `TextPainter(..)..layout()..paint(canvas, at)`.
    pub fn draw_text(&mut self, text: &str, style: &ParagraphStyle, at: Offset) -> Size {
        self.draw_paragraph(text, &[], style, at, None)
    }

    /// Draw a (rich) paragraph: `spans` style byte ranges of `text` over `style`, wrapped
    /// at `max_width` when `style.soft_wrap`, top-left at `at` (local). Returns its size.
    /// Flutter: `canvas.drawParagraph(paragraph..layout(width), at)`.
    pub fn draw_paragraph(
        &mut self,
        text: &str,
        spans: &[TextSpanStyle],
        style: &ParagraphStyle,
        at: Offset,
        max_width: Option<f64>,
    ) -> Size {
        let layout = crate::text::layout_paragraph(self.text, text, spans, style, max_width);
        let base = TextPaint::base(style);
        let brushes: SmallVec<[TextPaint; 4]> =
            std::iter::once(base.clone()).chain(spans.iter().map(|s| base.for_span(s))).collect();
        crate::text::paint::paint_into(
            &mut self.scene,
            self.text,
            self.visible,
            &layout,
            self.origin + at,
            &brushes,
        );
        let (w, h) = layout.size();
        Size::new(f64::from(w), f64::from(h))
    }

    /// Measure `text` in `style` without drawing (wraps at `max_width` when
    /// `style.soft_wrap`) — the size [`draw_paragraph`](Canvas::draw_paragraph) would return.
    pub fn measure_text(&mut self, text: &str, style: &ParagraphStyle, max_width: Option<f64>) -> Size {
        self.text.measure_text(text, style, max_width)
    }

    /// The monospace grid cell (advance × line height) of `family` at `font_size` /
    /// `line_height`, measured with this window's fonts (see
    /// [`TextEnv::monospace_cell`](crate::TextEnv::monospace_cell)).
    pub fn monospace_cell(&mut self, family: &str, font_size: f32, line_height: f32) -> Size {
        self.text.monospace_cell(family, font_size, line_height)
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
        let visible = cx.visible();
        let mut canvas =
            Canvas { scene: cx.scene.reborrow(), text: &mut *cx.text, origin: offset, size, visible };
        (self.painter)(&mut canvas);
    }

    /// The paint closure may read app state no invalidation tracks: never replay it.
    fn paint_is_volatile(&self) -> bool {
        true
    }

    fn debug_name(&self) -> &'static str {
        "RenderCanvas"
    }
}
