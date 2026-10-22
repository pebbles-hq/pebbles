//! Fuzzing entry points (11 S31). NOT public API — no semver guarantees.

use std::rc::Rc;
use std::sync::Arc;

use crate::objects::ParagraphStyle;
use crate::text::layout::{LayoutParams, TextLayout};
use crate::text::style::TextStyleSpec;
use crate::text::{TextEnv, fonts::FontRegistry, shape::shape_text};

fn glyph_count(layout: &TextLayout) -> usize {
    layout.lines.iter().map(|l| l.layout.glyphs.len()).sum()
}

/// Register `font` (owned bytes) in a FRESH registry with no bundled or system fonts, shape `text` at `px`,
/// return the glyph count. Must never panic.
pub fn shape_font_bytes(font: &[u8], text: &str, px: f32) -> usize {
    let px = if px.is_finite() { px.clamp(1.0, 1024.0) } else { 16.0 };
    let mut fonts = FontRegistry::empty();
    if fonts.register(Arc::new(font.to_vec())) == 0 {
        return 0;
    }
    let family = fonts.families().first().cloned();
    let style = ParagraphStyle { font_size: px, font_family: family, ..ParagraphStyle::default() };
    let spec = TextStyleSpec::from(&style);
    let shaped = Rc::new(shape_text(&mut fonts, text, &spec, &[]));
    let params =
        LayoutParams { width: None, wrap: false, align: None, max_lines: None, ellipsis: false, strut: None };
    let mut scratch = cosmic_text::ShapeBuffer::default();
    glyph_count(&TextLayout::new(shaped, params, &mut scratch, &mut fonts))
}

/// Lay out `text` with the deterministic bundled-only TextEnv (DR-15), wrapping at `max_width`;
/// return the glyph count. Must never panic.
pub fn layout_bundled(text: &str, max_width: f32) -> usize {
    let w = if max_width.is_finite() { max_width.clamp(1.0, 100_000.0) } else { 600.0 };
    let layout =
        crate::text::layout_for_test(&mut TextEnv::new(), text, &ParagraphStyle::default(), &[], Some(w));
    glyph_count(&layout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_font_shapes() {
        let inter = crate::fonts::builtin_fonts()[0].1;
        assert!(shape_font_bytes(inter, "Hello", 16.0) > 0);
        assert!(layout_bundled("Hello world", 100.0) > 0);
    }

    #[test]
    fn garbage_returns_zero() {
        assert_eq!(shape_font_bytes(&[], "Hello", 16.0), 0);
        let noise: Vec<u8> = (0u8..16).map(|i| i.wrapping_mul(97).wrapping_add(13)).collect();
        assert_eq!(shape_font_bytes(&noise, "Hello", f32::NAN), 0);
    }
}
