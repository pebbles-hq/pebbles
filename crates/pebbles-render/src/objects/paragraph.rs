//! [`RenderParagraph`] — a leaf render object that shapes text with parley and
//! paints its glyph runs into the vello scene.
//!
//! Layout builds and caches a `parley::Layout` (paint has no font context, so the
//! shaped layout must survive from layout to paint). Text is laid out in **logical**
//! pixels at scale 1.0; the shell applies the device-scale transform to the whole
//! scene, and because vello rasterizes glyph outlines through that transform the
//! text stays crisp at any DPI.

use pebbles_foundation::{Color, Offset, Size};
use parley::{
    Alignment, AlignmentOptions, FontWeight, Layout, LineHeight, PositionedLayoutItem,
    StyleProperty,
};
use vello::Glyph;
use vello::kurbo::Affine;
use vello::peniko::{Brush, Fill};

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// Styling for a paragraph of text.
#[derive(Clone, Debug)]
pub struct ParagraphStyle {
    pub font_size: f32,
    pub color: Color,
    /// Line height as a multiple of the font size.
    pub line_height: f32,
    /// Font weight (400 = normal, 600 = semibold, 700 = bold).
    pub weight: f32,
}

impl Default for ParagraphStyle {
    fn default() -> Self {
        ParagraphStyle {
            font_size: 16.0,
            color: pebbles_foundation::palette::BLACK,
            line_height: 1.2,
            weight: 400.0,
        }
    }
}

/// A leaf render object that lays out and paints text.
pub struct RenderParagraph {
    pub text: String,
    pub style: ParagraphStyle,
    /// Shaped layout, produced in [`RenderObject::layout`] and consumed in paint.
    cached: Option<Layout<Brush>>,
}

impl RenderParagraph {
    pub fn new(text: impl Into<String>, style: ParagraphStyle) -> Self {
        RenderParagraph { text: text.into(), style, cached: None }
    }
}

impl RenderObject for RenderParagraph {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        let max_advance =
            if constraints.has_bounded_width() { Some(constraints.max_width as f32) } else { None };

        let brush = Brush::Solid(self.style.color);
        let mut builder =
            cx.text.layout.ranged_builder(&mut cx.text.fonts, &self.text, 1.0, true);
        builder.push_default(StyleProperty::FontSize(self.style.font_size));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(self.style.weight)));
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
            self.style.line_height,
        )));
        builder.push_default(StyleProperty::Brush(brush));

        let mut layout: Layout<Brush> = builder.build(&self.text);
        layout.break_all_lines(max_advance);
        layout.align(Alignment::Start, AlignmentOptions::default());

        let size = Size::new(layout.width() as f64, layout.height() as f64);
        self.cached = Some(layout);
        constraints.constrain(size)
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        let Some(layout) = &self.cached else { return };
        let transform = Affine::translate((offset.x, offset.y));

        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let synthesis = run.synthesis();
                let glyph_transform = synthesis
                    .skew()
                    .map(|angle| Affine::skew(angle.to_radians().tan() as f64, 0.0));

                cx.scene
                    .draw_glyphs(run.font())
                    .brush(&glyph_run.style().brush)
                    .transform(transform)
                    .glyph_transform(glyph_transform)
                    .font_size(run.font_size())
                    .normalized_coords(run.normalized_coords())
                    .draw(
                        Fill::NonZero,
                        glyph_run.positioned_glyphs().map(|g| Glyph { id: g.id, x: g.x, y: g.y }),
                    );
            }
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderParagraph"
    }
}
