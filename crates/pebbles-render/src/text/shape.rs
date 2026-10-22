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

/// CSS word-separator characters (CSS Text 3 §8.2).
fn is_word_separator(c: char) -> bool {
    matches!(c, ' ' | '\u{A0}' | '\u{1361}' | '\u{10100}' | '\u{10101}' | '\u{1039F}' | '\u{1091F}')
}

/// Add word spacing to every word-separator glyph's advance before line breaking, so wraps see
/// it (Flutter `wordSpacing`). The last span covering a glyph with an override wins.
fn apply_word_spacing(
    shape: &mut ShapeLine,
    text: &str,
    para_start: usize,
    spec: &TextStyleSpec,
    spans: &[SpanSpec],
) {
    if spec.word_spacing == 0.0 && spans.iter().all(|s| s.word_spacing.is_none()) {
        return;
    }
    let base_size = sanitize_size(spec.font_size);
    for g in shape.spans.iter_mut().flat_map(|s| s.words.iter_mut()).flat_map(|w| w.glyphs.iter_mut()) {
        let at = para_start + g.start;
        let cluster = text.get(at..para_start + g.end).unwrap_or("");
        if cluster.is_empty() || !cluster.chars().all(is_word_separator) {
            continue;
        }
        let spacing = spans
            .iter()
            .rev()
            .find(|s| s.range.contains(&at) && s.word_spacing.is_some())
            .and_then(|s| s.word_spacing)
            .unwrap_or(spec.word_spacing);
        let size = g.metrics_opt.map_or(base_size, |m| m.font_size);
        if spacing.is_finite() && size > 0.0 {
            g.x_advance += spacing / size; // advances are in EM
        }
    }
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
            let mut shape = ShapeLine::new(fonts.font_system(), line, &attrs, Shaping::Advanced, tab_width);
            if !placeholder {
                apply_word_spacing(&mut shape, text, p.range.start, spec, spans);
            }
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

    /// Sum of the EM advances of the glyphs before byte `end`.
    fn advance_before(shaped: &ShapedText, end: usize) -> f32 {
        let line = &shaped.paras[0].shape;
        line.spans
            .iter()
            .flat_map(|s| &s.words)
            .flat_map(|w| &w.glyphs)
            .filter(|g| g.end <= end)
            .map(|g| g.x_advance)
            .sum()
    }

    fn mono(tab_width: u16) -> TextStyleSpec {
        TextStyleSpec::from(&ParagraphStyle {
            font_size: 20.0,
            font_family: Some("JetBrains Mono".into()),
            tab_width,
            ..ParagraphStyle::default()
        })
    }

    #[test]
    fn tab_is_one_column_in_mono() {
        let shaped = shape_text(&mut fonts(), "a\tb", &mono(1), &[]);
        let em = advance_before(&shaped, 2);
        assert!((em - 1.2).abs() <= 0.0001, "{em}");
    }

    #[test]
    fn tab_width_4_in_mono() {
        let shaped = shape_text(&mut fonts(), "a\tb", &mono(4), &[]);
        let em = advance_before(&shaped, 2);
        assert!((em - 2.4).abs() <= 0.0001, "{em}");
    }

    #[test]
    #[cfg(debug_assertions)] // the shape/layout counters are debug-only
    fn shape_counter_counts_shapelines() {
        let mut f = fonts();
        let before = crate::text::shape_count();
        shape_text(&mut f, "a\nb\nc", &spec(), &[]);
        assert_eq!(crate::text::shape_count() - before, 3);
    }

    fn width(text: &str, word_spacing: f32, max: Option<f32>) -> (f32, usize) {
        let mut env = crate::text::TextEnv::new();
        let style = ParagraphStyle {
            font_family: Some("JetBrains Mono".into()),
            font_size: 20.0,
            word_spacing,
            ..ParagraphStyle::default()
        };
        let l = crate::text::layout_for_test(&mut env, text, &style, &[], max);
        (l.width(), l.line_count())
    }

    #[test]
    fn word_spacing_widens_spaces_only() {
        assert!((width("a b", 5.0, None).0 - 41.0).abs() < 0.01, "{:?}", width("a b", 5.0, None));
        assert!((width("ab", 5.0, None).0 - 24.0).abs() < 0.01, "letters unchanged");
    }

    #[test]
    fn word_spacing_moves_wrap_points() {
        // "aaa bbb" = 84 px: fits 90 without spacing, wraps with +10 px.
        assert_eq!(width("aaa bbb", 0.0, Some(90.0)).1, 1);
        assert_eq!(width("aaa bbb", 10.0, Some(90.0)).1, 2);
    }

    #[test]
    fn negative_word_spacing_allowed() {
        assert!((width("a b", -4.0, None).0 - 32.0).abs() < 0.01, "{:?}", width("a b", -4.0, None));
    }
}
