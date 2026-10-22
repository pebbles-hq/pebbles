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
    /// Inline boxes in text order with the absolute byte of their U+FFFC (T48).
    pub(crate) placeholders: Vec<(usize, crate::objects::Placeholder)>,
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

/// The character standing in for an inline placeholder.
pub(crate) const OBJECT_REPLACEMENT: &str = "\u{FFFC}";

/// Each U+FFFC advances by its placeholder's width (in EM of the glyph's size); unmatched
/// U+FFFC (more characters than placeholders) keep their own advance.
fn apply_placeholder_advances(shape: &mut ShapeLine, text: &str, para_start: usize, spec: &TextStyleSpec) {
    if spec.placeholders.is_empty() {
        return;
    }
    let base_size = sanitize_size(spec.font_size);
    for g in shape.spans.iter_mut().flat_map(|s| s.words.iter_mut()).flat_map(|w| w.glyphs.iter_mut()) {
        let at = para_start + g.start;
        if text.get(at..para_start + g.end) != Some(OBJECT_REPLACEMENT) {
            continue;
        }
        let index = text[..at].matches(OBJECT_REPLACEMENT).count();
        if let Some(p) = spec.placeholders.get(index) {
            let size = g.metrics_opt.map_or(base_size, |m| m.font_size);
            if size > 0.0 && p.width.is_finite() {
                g.x_advance = p.width.max(0.0) / size;
            }
        }
    }
}

/// Variation axes other than `wght` change advances, but cosmic-text shapes at the `wght`-only
/// location: re-measure each affected glyph's advance with skrifa at the full location. GPOS
/// adjustments computed by harfrust at the `wght`-only location are kept (RK-30).
fn apply_variation_advances(
    fonts: &mut FontRegistry,
    shape: &mut ShapeLine,
    para_start: usize,
    spec: &TextStyleSpec,
    spans: &[SpanSpec],
) {
    use cosmic_text::skrifa::{FontRef, GlyphId, MetadataProvider, instance::Size};
    let affects = |v: &[crate::objects::FontVariation]| v.iter().any(|v| &v.axis != b"wght");
    if !affects(&spec.variations) && !spans.iter().any(|s| affects(&s.variations)) {
        return;
    }
    for g in shape.spans.iter_mut().flat_map(|s| s.words.iter_mut()).flat_map(|w| w.glyphs.iter_mut()) {
        let at = para_start + g.start;
        let vars = spans
            .iter()
            .rev()
            .find(|s| s.range.contains(&at) && !s.variations.is_empty())
            .map_or(&spec.variations[..], |s| &s.variations[..]);
        if !affects(vars) {
            continue;
        }
        let Some(face) = fonts.face_with(g.font_id, g.font_weight.0, vars) else { continue };
        let Ok(font_ref) = FontRef::from_index(face.font.data.data(), face.font.index) else { continue };
        let coords: Vec<cosmic_text::skrifa::instance::NormalizedCoord> = face
            .coords
            .iter()
            .map(|&c| cosmic_text::skrifa::instance::NormalizedCoord::from_bits(c))
            .collect();
        let metrics = font_ref.glyph_metrics(Size::new(1.0), coords.as_slice());
        if let Some(em) = metrics.advance_width(GlyphId::new(u32::from(g.glyph_id))) {
            g.x_advance = em; // EM units at size 1
        }
    }
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
                apply_placeholder_advances(&mut shape, text, p.range.start, spec);
                apply_variation_advances(fonts, &mut shape, p.range.start, spec, spans);
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
        placeholders: text
            .match_indices(OBJECT_REPLACEMENT)
            .map(|(b, _)| b)
            .zip(spec.placeholders.iter().copied())
            .collect(),
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

    use crate::objects::{FontFeature, FontVariation};

    fn lay(text: &str, family: &str, f: impl FnOnce(&mut ParagraphStyle)) -> Rc<crate::text::TextLayout> {
        let mut style =
            ParagraphStyle { font_family: Some(family.into()), font_size: 20.0, ..ParagraphStyle::default() };
        f(&mut style);
        crate::text::layout_for_test(&mut crate::text::TextEnv::new(), text, &style, &[], None)
    }

    #[test]
    fn tnum_makes_digits_equal_width() {
        let w = |t: &str, tnum: bool| {
            lay(t, "Inter", |s| {
                if tnum {
                    s.font_features = vec![FontFeature { tag: *b"tnum", value: 1 }];
                }
            })
            .width()
        };
        assert!((w("1111", false) - w("0000", false)).abs() > 0.5, "proportional digits differ");
        assert!((w("1111", true) - w("0000", true)).abs() < 0.01, "tabular digits are equal");
    }

    #[test]
    fn liga_off_splits_fi() {
        let glyphs = |liga: Option<u32>| {
            let l = lay("fi", "Lora", |s| {
                if let Some(v) = liga {
                    s.font_features = vec![FontFeature { tag: *b"liga", value: v }];
                }
            });
            l.lines[0].layout.glyphs.len()
        };
        assert_eq!(glyphs(None), 1, "Lora forms the fi ligature by default");
        assert_eq!(glyphs(Some(0)), 2);
    }

    #[test]
    fn opsz_axis_changes_advances() {
        let w = |opsz: f32| {
            lay("Hamburgefonstiv", "Inter", |s| {
                s.font_variations = vec![FontVariation { axis: *b"opsz", value: opsz }]
            })
            .width()
        };
        assert!((w(32.0) - w(14.0)).abs() > 0.1, "{} vs {}", w(32.0), w(14.0));
    }

    #[test]
    fn coords_reach_glyph_ops() {
        use crate::paint::{Painter, Scene};
        use crate::text::paint::{TextPaint, paint_into};
        let mut env = crate::text::TextEnv::new();
        let style = ParagraphStyle {
            font_family: Some("Inter".into()),
            font_variations: vec![FontVariation { axis: *b"opsz", value: 32.0 }],
            ..ParagraphStyle::default()
        };
        let layout = crate::text::layout_for_test(&mut env, "Hi", &style, &[], None);
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        let all = kurbo::Rect::new(-1e9, -1e9, 1e9, 1e9);
        paint_into(
            &mut p,
            &mut env,
            all,
            &layout,
            pebbles_foundation::Offset::new(0.0, 0.0),
            &[TextPaint::base(&style)],
        );
        let coords = scene.ops().iter().find_map(|op| match op {
            crate::paint::display_list::Op::Glyphs { normalized_coords, .. } => {
                Some(normalized_coords.clone())
            }
            _ => None,
        });
        let coords = coords.expect("a glyph op");
        assert_eq!(coords.len(), 2, "opsz and wght");
        assert_ne!(coords[0], 0, "opsz moved off its default");
    }

    #[test]
    fn feature_and_variation_sets_split_the_cache() {
        let spec = |f: Vec<FontFeature>, v: Vec<FontVariation>| TextStyleSpec {
            features: f,
            variations: v,
            ..TextStyleSpec::from(&ParagraphStyle::default())
        };
        let h = |s: &TextStyleSpec| crate::text::style::shape_hash("ab", s, &[], 1);
        let base = h(&spec(vec![], vec![]));
        assert_ne!(base, h(&spec(vec![FontFeature { tag: *b"tnum", value: 1 }], vec![])));
        assert_ne!(base, h(&spec(vec![], vec![FontVariation { axis: *b"opsz", value: 32.0 }])));
    }
}
