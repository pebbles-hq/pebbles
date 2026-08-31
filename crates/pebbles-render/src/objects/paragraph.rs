//! [`RenderParagraph`] — a leaf render object that shapes text with parley and
//! paints its glyph runs into the vello scene.
//!
//! Layout builds and caches a `parley::Layout` (paint has no font context, so the
//! shaped layout must survive from layout to paint). Text is laid out in **logical**
//! pixels at scale 1.0; the shell applies the device-scale transform to the whole
//! scene, and because vello rasterizes glyph outlines through that transform the
//! text stays crisp at any DPI.

use pebbles_foundation::{Color, Offset, Size, TextAlign};
use parley::{
    Alignment, AlignmentOptions, FontStyle, FontWeight, Layout, LineHeight, PositionedLayoutItem,
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
    /// Horizontal alignment within the paragraph's width.
    pub align: TextAlign,
    /// Extra spacing between letters (logical px; 0 = none).
    pub letter_spacing: f32,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    /// A font family name (system fallback if unset/unavailable).
    pub font_family: Option<String>,
    /// Clamp the paragraph to at most this many lines (excess lines are dropped).
    pub max_lines: Option<u32>,
}

impl Default for ParagraphStyle {
    fn default() -> Self {
        ParagraphStyle {
            font_size: 16.0,
            color: pebbles_foundation::palette::BLACK,
            line_height: 1.2,
            weight: 400.0,
            align: TextAlign::Start,
            letter_spacing: 0.0,
            italic: false,
            underline: false,
            strikethrough: false,
            font_family: None,
            max_lines: None,
        }
    }
}

fn to_parley_align(a: TextAlign) -> Alignment {
    match a {
        TextAlign::Left => Alignment::Left,
        TextAlign::Right => Alignment::Right,
        TextAlign::Center => Alignment::Center,
        TextAlign::Justify => Alignment::Justify,
        TextAlign::Start => Alignment::Start,
        TextAlign::End => Alignment::End,
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
        if self.style.letter_spacing != 0.0 {
            builder.push_default(StyleProperty::LetterSpacing(self.style.letter_spacing));
        }
        if self.style.italic {
            builder.push_default(StyleProperty::FontStyle(FontStyle::Italic));
        }
        if self.style.underline {
            builder.push_default(StyleProperty::Underline(true));
        }
        if self.style.strikethrough {
            builder.push_default(StyleProperty::Strikethrough(true));
        }
        if let Some(family) = &self.style.font_family {
            builder.push_default(StyleProperty::FontFamily(family.as_str().into()));
        }

        let mut layout: Layout<Brush> = builder.build(&self.text);
        layout.break_all_lines(max_advance);
        layout.align(to_parley_align(self.style.align), AlignmentOptions::default());

        // Clamp to `max_lines`: report a height covering only the kept lines; paint
        // skips the rest. (v1 truncation — no ellipsis re-shaping yet.)
        let full_h = layout.height() as f64;
        let height = match self.style.max_lines {
            Some(max) => {
                let kept = layout.lines().take(max as usize);
                let mut h = 0.0f64;
                for line in kept {
                    h += line.metrics().line_height as f64;
                }
                if h > 0.0 { h.min(full_h) } else { full_h }
            }
            None => full_h,
        };
        let size = Size::new(layout.width() as f64, height);
        self.cached = Some(layout);
        constraints.constrain(size)
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        let Some(layout) = &self.cached else { return };
        let transform = Affine::translate((offset.x, offset.y));

        let max_lines = self.style.max_lines.map(|m| m as usize).unwrap_or(usize::MAX);
        for line in layout.lines().take(max_lines) {
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
