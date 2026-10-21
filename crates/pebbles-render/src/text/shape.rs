//! Paragraph splitting and cosmic-text shaping (03 §3.6). Every BidiClass-B separator ends a
//! paragraph, so each `ShapeLine` holds exactly one bidi paragraph (cosmic-text panics on a
//! line mixing paragraph directions).

#![allow(dead_code)] // TextLayout (T10) consumes ShapedText.

use std::ops::Range;
use std::rc::Rc;

use cosmic_text::{AttrsList, ShapeLine, Shaping};

use super::fonts::FontRegistry;
use super::style::{SpanSpec, TextStyleSpec, attrs_list_for, sanitize_lh, sanitize_size};

/// One paragraph of a text: its byte range (excluding the separator) and separator length.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Para {
    pub(crate) range: Range<usize>,
    pub(crate) ending_len: u8,
}

pub(crate) struct ShapedPara {
    /// Paragraph text in `ShapedText::text`, EXCLUDING its ending.
    pub(crate) range: Range<usize>,
    pub(crate) ending_len: u8,
    /// Empty paragraph shaped as " " (never painted, never addressable).
    pub(crate) placeholder: bool,
    /// Kept to reshape the elided variant (§3.7.4).
    pub(crate) attrs: AttrsList,
    pub(crate) shape: ShapeLine,
}

pub(crate) struct ShapedText {
    pub(crate) text: Rc<str>,
    pub(crate) paras: Vec<ShapedPara>,
    pub(crate) tab_width: u16,
    /// Sanitized.
    pub(crate) base_font_size: f32,
    /// `base_font_size` × sanitized line height.
    pub(crate) base_line_height_px: f32,
    /// Font generation it was shaped with.
    pub(crate) generation: u64,
    /// (min, max) intrinsic width, T14.
    pub(crate) intrinsic: std::cell::Cell<Option<(f32, f32)>>,
}

/// Split `text` at every paragraph separator (§3.6 table). Text ending with a separator yields
/// a final empty paragraph; `""` yields one empty paragraph.
pub(crate) fn split_paragraphs(text: &str) -> Vec<Para> {
    let mut paras = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let ending_len = match c {
            '\r' if chars.peek().is_some_and(|&(_, n)| n == '\n') => {
                chars.next();
                2
            }
            '\r' | '\n' | '\u{1C}' | '\u{1D}' | '\u{1E}' => 1,
            '\u{85}' => 2,
            '\u{2029}' => 3,
            _ => continue,
        };
        paras.push(Para { range: start..i, ending_len });
        start = i + usize::from(ending_len);
    }
    paras.push(Para { range: start..text.len(), ending_len: 0 });
    paras
}

/// Shape every paragraph of `text` (Advanced shaping: font fallback included).
pub(crate) fn shape_text(
    fonts: &mut FontRegistry,
    text: &str,
    spec: &TextStyleSpec,
    spans: &[SpanSpec],
) -> ShapedText {
    let tab_width = spec.tab_width.max(1);
    let paras = split_paragraphs(text)
        .into_iter()
        .map(|p| {
            let placeholder = p.range.is_empty();
            let attrs = if placeholder {
                attrs_list_for(0..0, "", spec, &[], fonts)
            } else {
                attrs_list_for(p.range.clone(), text, spec, spans, fonts)
            };
            let line = if placeholder { " " } else { &text[p.range.clone()] };
            super::count_shape();
            let shape = ShapeLine::new(fonts.font_system(), line, &attrs, Shaping::Advanced, tab_width);
            ShapedPara { range: p.range, ending_len: p.ending_len, placeholder, attrs, shape }
        })
        .collect();
    let base_font_size = sanitize_size(spec.font_size);
    ShapedText {
        text: Rc::from(text),
        paras,
        tab_width,
        base_font_size,
        base_line_height_px: base_font_size * sanitize_lh(spec.line_height),
        generation: fonts.generation(),
        intrinsic: std::cell::Cell::new(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::ParagraphStyle;

    fn fonts() -> FontRegistry {
        FontRegistry::new_bundled(&[], "en-US".into())
    }

    fn spec() -> TextStyleSpec {
        TextStyleSpec::from(&ParagraphStyle::default())
    }

    #[test]
    fn split_table() {
        let split = |t: &str| -> Vec<(Range<usize>, u8)> {
            split_paragraphs(t).into_iter().map(|p| (p.range, p.ending_len)).collect()
        };
        assert_eq!(split("a\r\nb"), [(0..1, 2), (3..4, 0)]);
        assert_eq!(split("a\n"), [(0..1, 1), (2..2, 0)]);
        assert_eq!(split("a\rb"), [(0..1, 1), (2..3, 0)]);
        assert_eq!(split("a\u{85}b"), [(0..1, 2), (3..4, 0)]);
        assert_eq!(split("a\u{2029}b"), [(0..1, 3), (4..5, 0)]);
        assert_eq!(split("a\u{1E}b"), [(0..1, 1), (2..3, 0)]);
        assert_eq!(split(""), [(0..0, 0)]);
    }

    #[test]
    fn mixed_direction_separator_does_not_panic() {
        let shaped = shape_text(&mut fonts(), "abc\u{2029}\u{5d0}\u{5d1}\u{5d2}", &spec(), &[]);
        assert_eq!(shaped.paras.len(), 2);
        assert!(shaped.paras[1].shape.rtl);
    }

    #[test]
    fn zero_and_nan_sizes_do_not_panic() {
        for size in [0.0, f32::NAN] {
            let s = TextStyleSpec { font_size: size, ..spec() };
            shape_text(&mut fonts(), "hello", &s, &[]);
        }
    }

    #[test]
    fn empty_para_is_placeholder() {
        let shaped = shape_text(&mut fonts(), "a\n", &spec(), &[]);
        assert!(!shaped.paras[0].placeholder);
        assert!(shaped.paras[1].placeholder);
    }

    #[test]
    fn shape_counter_counts_shapelines() {
        let mut f = fonts();
        let before = crate::text::shape_count();
        shape_text(&mut f, "a\nb\nc", &spec(), &[]);
        assert_eq!(crate::text::shape_count() - before, 3);
    }
}
