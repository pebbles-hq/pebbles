//! [`RenderTextField`] — a leaf render object that paints editable text with a
//! selection highlight and a caret. It is display-only: the widget layer owns the
//! string and the selection (anchor + focus byte offsets) and feeds them in; this
//! object shapes the text with parley, paints the selection rects + glyphs + caret
//! (all via parley geometry), and publishes its `Layout` to [`crate::text_edit`] so
//! the widget can do cursor motion and hit-testing.

use std::rc::Rc;

use pebbles_foundation::{Color, Offset, Size};
use parley::{
    Affinity, Alignment, AlignmentOptions, Cursor, FontWeight, Layout, LineHeight,
    PositionedLayoutItem, Selection, StyleProperty,
};
use vello::Glyph;
use vello::kurbo::{Affine, Rect};
use vello::peniko::{Brush, Fill};

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// Visual styling for an editable text field.
#[derive(Clone, Debug)]
pub struct TextFieldStyle {
    pub font_size: f32,
    pub weight: f32,
    pub line_height: f32,
    /// Color of entered text.
    pub color: Color,
    /// Color of the placeholder (shown when empty).
    pub placeholder_color: Color,
    /// Color of the caret.
    pub caret_color: Color,
    /// Fill behind the selected range.
    pub selection_color: Color,
}

impl Default for TextFieldStyle {
    fn default() -> Self {
        TextFieldStyle {
            font_size: 14.0,
            weight: 400.0,
            line_height: 1.3,
            color: pebbles_foundation::palette::BLACK,
            placeholder_color: pebbles_foundation::palette::zinc::S400,
            caret_color: pebbles_foundation::palette::BLACK,
            selection_color: Color::from_rgba8(59, 130, 246, 70),
        }
    }
}

/// A leaf render object that paints editable text with a selection + caret.
pub struct RenderTextField {
    /// The entered text (source of truth lives in the widget layer).
    pub text: String,
    /// Placeholder shown when `text` is empty.
    pub placeholder: String,
    /// Selection anchor (byte offset into `text`).
    pub anchor: usize,
    /// Selection focus / caret (byte offset into `text`).
    pub focus: usize,
    /// Whether the field is focused (controls the caret + hit-test publishing).
    pub focused: bool,
    /// If set, every character renders as this glyph (password fields).
    pub obscure: Option<char>,
    /// Whether newlines stack (textarea) vs. a single line.
    pub multiline: bool,
    /// Stable id for publishing the layout to [`crate::text_edit`].
    pub field_id: u64,
    pub style: TextFieldStyle,
    /// Shaped display layout (Rc so it can be published + kept for paint).
    cached: Option<Rc<Layout<Brush>>>,
    line_px: f64,
}

impl RenderTextField {
    pub fn new(text: impl Into<String>, style: TextFieldStyle) -> Self {
        RenderTextField {
            text: text.into(),
            placeholder: String::new(),
            anchor: 0,
            focus: 0,
            focused: false,
            obscure: None,
            multiline: false,
            field_id: 0,
            style,
            cached: None,
            line_px: 0.0,
        }
    }

    /// The string actually painted (obscured for passwords, placeholder if empty).
    fn display_text(&self) -> (String, Color, bool) {
        if self.text.is_empty() {
            (self.placeholder.clone(), self.style.placeholder_color, true)
        } else if let Some(ch) = self.obscure {
            (std::iter::repeat_n(ch, self.text.chars().count()).collect(), self.style.color, false)
        } else {
            (self.text.clone(), self.style.color, false)
        }
    }

    /// Map a real-text byte offset onto the display string (identity unless obscured).
    fn to_display(&self, byte: usize) -> usize {
        let byte = byte.min(self.text.len());
        match self.obscure {
            Some(ch) => self.text[..byte].chars().count() * ch.len_utf8(),
            None => byte,
        }
    }
}

impl RenderObject for RenderTextField {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        let max_advance = if self.multiline && constraints.has_bounded_width() {
            Some(constraints.max_width as f32)
        } else {
            None
        };
        let (display, color, _placeholder) = self.display_text();

        let mut builder = cx.text.layout.ranged_builder(&mut cx.text.fonts, &display, 1.0, true);
        builder.push_default(StyleProperty::FontSize(self.style.font_size));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(self.style.weight)));
        builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(
            self.style.line_height,
        )));
        builder.push_default(StyleProperty::Brush(Brush::Solid(color)));
        let mut layout: Layout<Brush> = builder.build(&display);
        layout.break_all_lines(max_advance);
        layout.align(Alignment::Start, AlignmentOptions::default());

        self.line_px = (self.style.font_size as f64) * (self.style.line_height as f64);
        let height = (layout.height() as f64).max(self.line_px);
        let width =
            if constraints.has_bounded_width() { constraints.max_width } else { layout.width() as f64 };

        let layout = Rc::new(layout);
        // Publish for hit-testing / motion — but only real (unobscured) text.
        if self.obscure.is_none() && !self.text.is_empty() {
            crate::text_edit::store(self.field_id, layout.clone());
        } else {
            crate::text_edit::clear(self.field_id);
        }
        self.cached = Some(layout);
        constraints.constrain(Size::new(width, height))
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        let Some(layout) = &self.cached else { return };
        let transform = Affine::translate((offset.x, offset.y));

        // 1. Selection highlight (skip for password / placeholder).
        let has_text = !self.text.is_empty();
        if has_text && self.obscure.is_none() && self.anchor != self.focus {
            let a = self.to_display(self.anchor);
            let f = self.to_display(self.focus);
            let sel = Selection::new(
                Cursor::from_byte_index(layout, a, Affinity::Downstream),
                Cursor::from_byte_index(layout, f, Affinity::Downstream),
            );
            for (bb, _) in sel.geometry(layout) {
                let rect = Rect::new(
                    offset.x + bb.x0,
                    offset.y + bb.y0,
                    offset.x + bb.x1,
                    offset.y + bb.y1,
                );
                cx.scene.fill(Fill::NonZero, Affine::IDENTITY, self.style.selection_color, None, &rect);
            }
        }

        // 2. Glyphs.
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

        // 3. Caret at the focus (when focused).
        if self.focused {
            let f = if has_text { self.to_display(self.focus) } else { 0 };
            let bb = Cursor::from_byte_index(layout, f, Affinity::Downstream).geometry(layout, 1.5);
            let rect = Rect::new(
                offset.x + bb.x0,
                offset.y + bb.y0 + 1.0,
                offset.x + bb.x0 + 1.5,
                offset.y + bb.y1 - 1.0,
            );
            cx.scene.fill(Fill::NonZero, Affine::IDENTITY, self.style.caret_color, None, &rect);
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderTextField"
    }
}
