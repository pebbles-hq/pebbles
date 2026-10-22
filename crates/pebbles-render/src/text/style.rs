//! Pebbles text styles -> cosmic-text attributes (03 §3.5): sanitized specs, layered span
//! flattening (a later span wins per property, pre-swap parity) and the shape-cache hash
//! (§3.8; colors, width and alignment never enter it).

#![allow(dead_code)] // paragraphs and fields shape through this from T10/T22.

use std::hash::{Hash, Hasher};
use std::ops::Range;

use cosmic_text::{Attrs, AttrsList, Metrics, Style, UnderlineStyle, Weight};
use pebbles_foundation::TextAlign;

use super::fonts::FontRegistry;
use crate::objects::{ParagraphStyle, TextFieldStyle, TextSpanStyle};

/// TAB advance in space widths (`ParagraphStyle::tab_width` default; text fields always).
pub const DEFAULT_TAB_WIDTH: u16 = 1;

/// The shaping-relevant part of a paragraph or field style.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextStyleSpec {
    pub(crate) font_size: f32,
    pub(crate) line_height: f32,
    pub(crate) weight: f32,
    pub(crate) italic: bool,
    pub(crate) underline: bool,
    pub(crate) strikethrough: bool,
    pub(crate) overline: bool,
    pub(crate) letter_spacing: f32,
    pub(crate) word_spacing: f32,
    pub(crate) family: Option<String>,
    pub(crate) tab_width: u16,
}

/// The shaping-relevant part of a span (chip/link are geometry-only and ignored).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SpanSpec {
    pub(crate) range: Range<usize>,
    pub(crate) weight: Option<f32>,
    pub(crate) italic: bool,
    pub(crate) underline: bool,
    pub(crate) strikethrough: bool,
    pub(crate) overline: bool,
    /// The span has its own paint slot (color or decoration override).
    pub(crate) has_color: bool,
    pub(crate) family: Option<String>,
    pub(crate) size: Option<f32>,
    pub(crate) word_spacing: Option<f32>,
}

impl From<&ParagraphStyle> for TextStyleSpec {
    fn from(s: &ParagraphStyle) -> Self {
        TextStyleSpec {
            font_size: s.font_size,
            line_height: s.line_height,
            weight: s.weight,
            italic: s.italic,
            underline: s.underline,
            strikethrough: s.strikethrough,
            overline: s.overline,
            letter_spacing: s.letter_spacing,
            word_spacing: s.word_spacing,
            family: s.font_family.clone(),
            tab_width: s.tab_width.max(1),
        }
    }
}

impl TextStyleSpec {
    /// A text field's spec (default family, no decorations).
    pub(crate) fn for_field(s: &TextFieldStyle) -> Self {
        TextStyleSpec {
            font_size: s.font_size,
            line_height: s.line_height,
            weight: s.weight,
            italic: false,
            underline: false,
            strikethrough: false,
            overline: false,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            family: None,
            tab_width: DEFAULT_TAB_WIDTH,
        }
    }
}

impl From<&TextSpanStyle> for SpanSpec {
    fn from(s: &TextSpanStyle) -> Self {
        SpanSpec {
            range: s.range.clone(),
            weight: s.weight,
            italic: s.italic,
            underline: s.underline,
            strikethrough: s.strikethrough,
            overline: s.overline,
            // A span with its own colors or decoration paint needs its own paint slot.
            has_color: s.color.is_some()
                || !s.shadows.is_empty()
                || s.decoration_color.is_some()
                || s.decoration_style.is_some()
                || s.decoration_thickness.is_some(),
            family: s.family.clone(),
            size: s.size,
            word_spacing: s.word_spacing,
        }
    }
}

/// Non-finite -> 16; negative -> 0; above 4096 -> 4096. Size 0 is allowed (zero-height layout).
pub(crate) fn sanitize_size(size: f32) -> f32 {
    if size.is_finite() { size.clamp(0.0, 4096.0) } else { 16.0 }
}

/// Non-finite -> 1.2; clamped to 0..=100.
pub(crate) fn sanitize_lh(lh: f32) -> f32 {
    if lh.is_finite() { lh.clamp(0.0, 100.0) } else { 1.2 }
}

/// Rounded and clamped to 1..=1000; NaN -> 400.
pub(crate) fn weight_of(w: f32) -> Weight {
    if w.is_nan() { Weight(400) } else { Weight(w.round().clamp(1.0, 1000.0) as u16) }
}

/// `TextAlign` + ambient direction -> cosmic alignment (§3.5.1).
pub(crate) fn to_cosmic_align(a: TextAlign, rtl: bool) -> cosmic_text::Align {
    use cosmic_text::Align;
    match a {
        TextAlign::Left => Align::Left,
        TextAlign::Right => Align::Right,
        TextAlign::Center => Align::Center,
        TextAlign::Justify => Align::Justified,
        TextAlign::Start => {
            if rtl {
                Align::Right
            } else {
                Align::Left
            }
        }
        TextAlign::End => {
            if rtl {
                Align::Left
            } else {
                Align::Right
            }
        }
    }
}

/// One flattened segment's merged properties.
#[derive(Clone, PartialEq)]
struct Merged<'s> {
    weight: f32,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    overline: bool,
    family: Option<&'s str>,
    size: f32,
    slot: usize,
}

impl<'s> Merged<'s> {
    fn base(spec: &'s TextStyleSpec) -> Self {
        Merged {
            weight: spec.weight,
            italic: spec.italic,
            underline: spec.underline,
            strikethrough: spec.strikethrough,
            overline: spec.overline,
            family: spec.family.as_deref(),
            size: spec.font_size,
            slot: 0,
        }
    }

    /// The cosmic attributes of this segment; `family` is its resolved family (borrowed).
    fn attrs<'f>(&self, spec: &TextStyleSpec, family: &'f cosmic_text::FamilyOwned) -> Attrs<'f> {
        let size = sanitize_size(self.size);
        let mut attrs = Attrs::new()
            .family(family.as_family())
            .weight(weight_of(self.weight))
            .style(if self.italic { Style::Italic } else { Style::Normal })
            .metrics(Metrics::relative(size, sanitize_lh(spec.line_height)))
            .metadata(self.slot);
        let ls = if spec.letter_spacing.is_finite() { spec.letter_spacing } else { 0.0 };
        if ls != 0.0 && size != 0.0 {
            attrs = attrs.letter_spacing(ls / size);
        }
        if self.underline {
            attrs = attrs.underline(UnderlineStyle::Single);
        }
        if self.strikethrough {
            attrs = attrs.strikethrough();
        }
        if self.overline {
            attrs = attrs.overline();
        }
        attrs
    }

    fn family(&self) -> &str {
        self.family.unwrap_or(crate::fonts::DEFAULT_FAMILY)
    }
}

/// Snap `b` down to a char boundary of `s`.
fn floor_boundary(s: &str, mut b: usize) -> usize {
    b = b.min(s.len());
    while !s.is_char_boundary(b) {
        b -= 1;
    }
    b
}

/// The attributes of paragraph `para` (a byte range of `text`): base defaults plus one span
/// per flattened segment that differs from the base (§3.5.2).
pub(crate) fn attrs_list_for(
    para: Range<usize>,
    text: &str,
    spec: &TextStyleSpec,
    spans: &[SpanSpec],
    fonts: &mut FontRegistry,
) -> AttrsList {
    let base = Merged::base(spec);
    let family = fonts.resolve_family(base.family());
    let mut list = AttrsList::new(&base.attrs(spec, &family));
    let slice = &text[para.clone()];
    let len = slice.len();
    // Rebased, clamped ranges; empty / out-of-text spans are skipped (§3.5.2 step 4).
    let rebased: Vec<(usize, Range<usize>)> = spans
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.range.is_empty() && s.range.start < text.len())
        .filter_map(|(i, s)| {
            let start = s.range.start.max(para.start).saturating_sub(para.start);
            let end = s.range.end.min(text.len()).min(para.end).saturating_sub(para.start);
            let (start, end) = (floor_boundary(slice, start.min(len)), floor_boundary(slice, end.min(len)));
            (start < end).then_some((i, start..end))
        })
        .collect();
    let mut bounds: Vec<usize> = vec![0, len];
    for (_, r) in &rebased {
        bounds.push(r.start);
        bounds.push(r.end);
    }
    bounds.sort_unstable();
    bounds.dedup();
    for w in bounds.windows(2) {
        let seg = w[0]..w[1];
        if seg.is_empty() {
            continue;
        }
        let mut merged = base.clone();
        for (i, r) in &rebased {
            if r.start > seg.start || r.end < seg.end {
                continue;
            }
            let s = &spans[*i];
            if let Some(weight) = s.weight {
                merged.weight = weight;
            }
            merged.italic |= s.italic;
            merged.underline |= s.underline;
            merged.strikethrough |= s.strikethrough;
            merged.overline |= s.overline;
            if let Some(family) = s.family.as_deref() {
                merged.family = Some(family);
            }
            if let Some(size) = s.size {
                merged.size = size;
            }
            if s.has_color {
                merged.slot = i + 1;
            }
        }
        if merged != base {
            let family = fonts.resolve_family(merged.family());
            list.add_span(seg, &merged.attrs(spec, &family));
        }
    }
    list
}

/// The shape-cache key (§3.8): text, sanitized spec without colors, spans, tab width and the
/// font generation — field by field, no formatting, no allocation.
pub(crate) fn shape_hash(text: &str, spec: &TextStyleSpec, spans: &[SpanSpec], generation: u64) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    0x5EA9E_u32.hash(&mut h);
    text.hash(&mut h);
    sanitize_size(spec.font_size).to_bits().hash(&mut h);
    sanitize_lh(spec.line_height).to_bits().hash(&mut h);
    weight_of(spec.weight).0.hash(&mut h);
    (spec.italic, spec.underline, spec.strikethrough, spec.overline).hash(&mut h);
    spec.letter_spacing.to_bits().hash(&mut h);
    spec.word_spacing.to_bits().hash(&mut h);
    spec.family.hash(&mut h);
    spec.tab_width.max(1).hash(&mut h);
    spans.len().hash(&mut h);
    for s in spans {
        s.range.hash(&mut h);
        s.weight.map(f32::to_bits).hash(&mut h);
        (s.italic, s.underline, s.strikethrough, s.overline, s.has_color).hash(&mut h);
        s.family.hash(&mut h);
        s.size.map(f32::to_bits).hash(&mut h);
        s.word_spacing.map(f32::to_bits).hash(&mut h);
    }
    generation.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic_text::LetterSpacing;

    fn spec(size: f32) -> TextStyleSpec {
        TextStyleSpec::from(&ParagraphStyle { font_size: size, ..ParagraphStyle::default() })
    }

    fn fonts() -> FontRegistry {
        FontRegistry::new_bundled(&[], "en-US".into())
    }

    fn span(range: Range<usize>) -> SpanSpec {
        SpanSpec::from(&TextSpanStyle::new(range))
    }

    #[test]
    fn letter_spacing_is_em() {
        let mut f = fonts();
        let mut s = spec(20.0);
        s.letter_spacing = 2.0;
        let list = attrs_list_for(0..4, "abcd", &s, &[], &mut f);
        assert_eq!(list.defaults().letter_spacing_opt, Some(LetterSpacing(0.1)));
        let mut zero = spec(0.0);
        zero.letter_spacing = 2.0;
        assert_eq!(attrs_list_for(0..4, "abcd", &zero, &[], &mut f).defaults().letter_spacing_opt, None);
    }

    #[test]
    fn weight_sanitized() {
        assert_eq!(weight_of(1500.0), Weight(1000));
        assert_eq!(weight_of(f32::NAN), Weight(400));
        assert_eq!(weight_of(650.4), Weight(650));
    }

    #[test]
    fn size_sanitized() {
        assert_eq!(sanitize_size(f32::NAN), 16.0);
        assert_eq!(sanitize_size(-3.0), 0.0);
        assert_eq!(sanitize_size(1e9), 4096.0);
        assert_eq!(sanitize_lh(f32::NAN), 1.2);
    }

    #[test]
    fn spans_flatten_layered() {
        let mut f = fonts();
        let text = "abcdefghijklmnopqrst";
        let mut a = span(0..10);
        a.weight = Some(700.0);
        let mut b = span(5..15);
        b.italic = true;
        let list = attrs_list_for(0..20, text, &spec(16.0), &[a, b], &mut f);
        let got: Vec<(Range<usize>, u16, bool)> =
            list.spans_iter().map(|(r, at)| (r.clone(), at.weight.0, at.style == Style::Italic)).collect();
        assert_eq!(got, [(0..5, 700, false), (5..10, 700, true), (10..15, 400, true)]);
    }

    #[test]
    fn span_size_sets_metrics() {
        let mut f = fonts();
        let mut big = span(0..3);
        big.size = Some(24.0);
        let list = attrs_list_for(0..6, "abcdef", &spec(16.0), &[big], &mut f);
        let (_, at) = list.spans_iter().next().expect("one span");
        assert_eq!(at.metrics_opt, Some(Metrics::relative(24.0, 1.2).into()));
    }

    #[test]
    fn color_value_not_in_shape_hash() {
        let s = spec(16.0);
        let mut red = TextSpanStyle::new(0..2);
        red.color = Some(pebbles_foundation::palette::RED);
        let mut blue = TextSpanStyle::new(0..2);
        blue.color = Some(pebbles_foundation::palette::BLUE);
        let plain = TextSpanStyle::new(0..2);
        let h = |t: &TextSpanStyle| shape_hash("ab", &s, &[SpanSpec::from(t)], 1);
        assert_eq!(h(&red), h(&blue));
        assert_ne!(h(&red), h(&plain));
    }

    #[test]
    fn align_mapping() {
        use cosmic_text::Align;
        let table = [
            (TextAlign::Left, Align::Left, Align::Left),
            (TextAlign::Right, Align::Right, Align::Right),
            (TextAlign::Center, Align::Center, Align::Center),
            (TextAlign::Justify, Align::Justified, Align::Justified),
            (TextAlign::Start, Align::Left, Align::Right),
            (TextAlign::End, Align::Right, Align::Left),
        ];
        for (a, ltr, rtl) in table {
            assert_eq!(to_cosmic_align(a, false), ltr, "{a:?} ltr");
            assert_eq!(to_cosmic_align(a, true), rtl, "{a:?} rtl");
        }
    }
}
