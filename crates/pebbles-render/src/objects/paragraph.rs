//! [`RenderParagraph`] — a leaf render object that shapes text with parley and
//! paints its glyph runs into the vello scene.
//!
//! Layout builds and caches a `parley::Layout` (paint has no font context, so the
//! shaped layout must survive from layout to paint). Text is laid out in **logical**
//! pixels at scale 1.0; the shell applies the device-scale transform to the whole
//! scene, and because vello rasterizes glyph outlines through that transform the
//! text stays crisp at any DPI.

use pebbles_foundation::{Axis, Color, Offset, Size, TextAlign};
use parley::{
    Alignment, AlignmentOptions, FontStyle, FontWeight, Layout, LineHeight, PositionedLayoutItem,
    StyleProperty,
};
use vello::Glyph;
use vello::kurbo::Affine;
use vello::peniko::{Brush, Fill};

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::text::TextEnv;
use crate::tree::{IntrinsicCx, LayoutCx, PaintCx};

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
    /// With `max_lines`, append "…" to the last kept line when the text overflows.
    pub ellipsis: bool,
    /// Break onto the next line when the text exceeds the available width
    /// (`true`, the default). `false` shapes a single unbounded line that clips to
    /// the box (the classic one-line label, combine with `ellipsis`).
    pub soft_wrap: bool,
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
            ellipsis: false,
            soft_wrap: true,
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

    /// Shape `s` into a broken, aligned layout with this paragraph's style.
    fn build(&self, text_env: &mut TextEnv, s: &str, max_advance: Option<f32>) -> Layout<Brush> {
        let mut builder = text_env.layout.ranged_builder(&mut text_env.fonts, s, 1.0, true);
        builder.push_default(StyleProperty::FontSize(self.style.font_size));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(self.style.weight)));
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
            self.style.line_height,
        )));
        builder.push_default(StyleProperty::Brush(Brush::Solid(self.style.color)));
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
        let mut layout: Layout<Brush> = builder.build(s);
        layout.break_all_lines(max_advance);
        layout.align(to_parley_align(self.style.align), AlignmentOptions::default());
        layout
    }
}

impl RenderObject for RenderParagraph {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        let max_advance = if self.style.soft_wrap && constraints.has_bounded_width() {
            Some(constraints.max_width as f32)
        } else {
            None
        };

        let text_env = &mut *cx.text;
        let mut layout = self.build(text_env, &self.text, max_advance);

        // `max_lines` clamp. With `ellipsis`, when the text overflows, re-shape the
        // longest character prefix that still fits in `max_lines` lines once "…" is
        // appended (binary search on the char boundary).
        let full_h = layout.height() as f64;
        let mut height = full_h;
        if let Some(max) = self.style.max_lines {
            let max = max as usize;
            let over = layout.lines().count() > max;
            if over && self.style.ellipsis && !self.text.is_empty() {
                let chars: Vec<usize> = self.text.char_indices().map(|(i, _)| i).collect();
                let (mut lo, mut hi, mut best) = (0usize, chars.len(), String::from("…"));
                while lo < hi {
                    let mid = (lo + hi).div_ceil(2);
                    let cut = chars.get(mid).copied().unwrap_or(self.text.len());
                    let candidate = format!("{}…", &self.text[..cut]);
                    let trial = self.build(text_env, &candidate, max_advance);
                    if trial.lines().count() <= max {
                        best = candidate;
                        lo = mid;
                    } else {
                        hi = mid - 1;
                    }
                }
                layout = self.build(text_env, &best, max_advance);
            }
            // Report a height covering only the kept lines; paint skips the rest.
            let h: f64 = layout.lines().take(max).map(|l| l.metrics().line_height as f64).sum();
            if h > 0.0 {
                height = h.min(layout.height() as f64);
            }
        }
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

    fn intrinsic(&self, cx: &mut IntrinsicCx, axis: Axis, cross_extent: f64) -> Option<f64> {
        match axis {
            // The widest unbreakable run — approximated as the widest
            // whitespace-separated token (Flutter uses the same word-boundary
            // notion for `getMinIntrinsicWidth`). Capped for pathological input.
            Axis::Horizontal => {
                let text_env = &mut *cx.text;
                let natural =
                    self.build(text_env, &self.text, None).width() as f64;
                let mut max_word: f64 = 0.0;
                for token in self.text.split_whitespace().take(256) {
                    let w = self.build(text_env, token, None).width() as f64;
                    max_word = max_word.max(w);
                }
                Some(if max_word > 0.0 { max_word } else { natural })
            }
            // The height the paragraph takes when wrapped at `cross_extent`.
            Axis::Vertical => {
                let max_advance = if cross_extent.is_finite() {
                    Some(cross_extent.max(0.0) as f32)
                } else {
                    None
                };
                Some(self.build(&mut *cx.text, &self.text, max_advance).height() as f64)
            }
        }
    }

    fn baseline(&self, _cx: &mut LayoutCx) -> Option<f64> {
        // The first line's baseline: the line's top offset plus its metrics'
        // baseline (the distance from the line top to the alphabetic baseline).
        self.cached.as_ref().and_then(|layout| {
            layout.lines().next().map(|line| {
                let m = line.metrics();
                (m.block_min_coord + m.baseline) as f64
            })
        })
    }

    fn debug_name(&self) -> &'static str {
        "RenderParagraph"
    }
}
