//! Laid-out text (03 §3.7): line breaking per paragraph through cosmic-text, cosmic's vertical
//! metrics, pre-swap-parity measurement (whitespace hanging at a soft wrap excluded, a
//! paragraph's trailing whitespace kept as in Flutter, justified text measured
//! unjustified, the empty last line of a text ending with a separator counted) and a common
//! alignment width for unbounded multi-paragraph text.

use std::rc::Rc;

use cosmic_text::{Ellipsize, Hinting, LayoutLine, ShapeBuffer, ShapeLine, Shaping, Wrap};

use super::fonts::FontRegistry;
use super::shape::{ShapedPara, ShapedText};

/// Line-breaking inputs (not shaping: width/align never enter the shape key).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LayoutParams {
    /// Wrap & align width (logical px); None = unbounded.
    pub(crate) width: Option<f32>,
    /// true = Wrap::Word, false = Wrap::None.
    pub(crate) wrap: bool,
    /// None = paragraph-direction start (text fields).
    pub(crate) align: Option<cosmic_text::Align>,
    pub(crate) max_lines: Option<u32>,
    pub(crate) ellipsis: bool,
    /// The paragraph's strut, resolved to px (Flutter `StrutStyle`).
    pub(crate) strut: Option<StrutMetrics>,
}

/// A resolved strut: the minimum extent above and below every baseline (px).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StrutMetrics {
    pub(crate) above: f32,
    pub(crate) below: f32,
    /// Every line is exactly the strut's extents (`force_strut_height`).
    pub(crate) force: bool,
}

pub(crate) struct VisualLine {
    /// Index into `ShapedText::paras`.
    pub(crate) para: usize,
    /// Glyph byte ranges index `Elided::text`, not the paragraph.
    pub(crate) from_elided: bool,
    pub(crate) layout: LayoutLine,
    pub(crate) top: f32,
    pub(crate) height: f32,
    /// Absolute y of the baseline in layout space.
    pub(crate) baseline: f32,
}

#[allow(dead_code)] // `cut` drives caret mapping of the elided line (T18+).
pub(crate) struct Elided {
    pub(crate) para: usize,
    pub(crate) cut: usize,
    pub(crate) text: String,
}

/// Immutable laid-out text. Cheap to share (`Rc`). Opaque outside pebbles-render.
pub struct TextLayout {
    pub(crate) shaped: Rc<ShapedText>,
    pub(crate) lines: Vec<VisualLine>,
    pub(crate) elided: Option<Elided>,
    #[allow(dead_code)] // the layout cache and intrinsics read it (T14/T17).
    pub(crate) params: LayoutParams,
    pub(crate) width: f32,
    pub(crate) height: f32,
    /// `params.width`, or the common width of pass 2 (§3.7.5).
    pub(crate) align_width: f32,
    /// Inline placeholder boxes in layout space, in placeholder order (T48).
    pub(crate) placeholder_boxes: Vec<kurbo::Rect>,
    /// `paras[kept_paras..]` were dropped by max_lines.
    #[allow(dead_code)] // caret/hit mapping clamps to kept paragraphs (T18).
    pub(crate) kept_paras: usize,
}

/// The layout-cache key (§3.8): the shape key plus every line-breaking input.
pub(crate) fn layout_hash(shape: u64, params: &LayoutParams) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    shape.hash(&mut h);
    params.width.map(f32::to_bits).hash(&mut h);
    params.wrap.hash(&mut h);
    let align_tag: u8 = match params.align {
        None => 0,
        Some(cosmic_text::Align::Left) => 1,
        Some(cosmic_text::Align::Right) => 2,
        Some(cosmic_text::Align::Center) => 3,
        Some(cosmic_text::Align::Justified) => 4,
        Some(cosmic_text::Align::End) => 5,
    };
    align_tag.hash(&mut h);
    params.max_lines.hash(&mut h);
    params.ellipsis.hash(&mut h);
    params.strut.map(|s| (s.above.to_bits(), s.below.to_bits(), s.force)).hash(&mut h);
    h.finish()
}

/// Break one shaped line into layout lines.
fn break_line(
    shaped: &ShapedText,
    shape: &ShapeLine,
    width: Option<f32>,
    wrap: bool,
    align: Option<cosmic_text::Align>,
    scratch: &mut ShapeBuffer,
) -> Vec<LayoutLine> {
    let mut out = Vec::new();
    shape.layout_to_buffer(
        scratch,
        shaped.base_font_size,
        width,
        if wrap { Wrap::Word } else { Wrap::None },
        Ellipsize::None,
        align,
        &mut out,
        None,
        Hinting::Disabled,
    );
    if out.is_empty() {
        // cosmic-text returns no line for a paragraph without glyphs; keep one empty line.
        out.push(LayoutLine {
            w: 0.0,
            max_ascent: 0.0,
            max_descent: 0.0,
            line_height_opt: None,
            glyphs: Vec::new(),
            decorations: Vec::new(),
        });
    }
    out
}

/// The elided variant of one paragraph: its shape and the kept prefix.
struct ElidedShape {
    para: usize,
    cut: usize,
    text: String,
    shape: ShapeLine,
}

/// ELIDE(i, k) (§3.7.4): the longest prefix of paragraph `i` that, followed by "…", lays out
/// in at most `k` lines (binary search over char starts, the pre-swap algorithm per paragraph).
/// The prefix keeps its span attributes; the "…" takes the defaults.
fn elide(
    shaped: &ShapedText,
    i: usize,
    k: usize,
    params: &LayoutParams,
    scratch: &mut ShapeBuffer,
    fonts: &mut FontRegistry,
) -> ElidedShape {
    let para = &shaped.paras[i];
    let para_text = &shaped.text[para.range.clone()];
    let chars: Vec<usize> = para_text.char_indices().map(|(b, _)| b).collect();
    let mut shape_prefix = |cut: usize| {
        let mut attrs = para.attrs.clone();
        let _ = attrs.split_off(cut);
        let text = format!("{}\u{2026}", &para_text[..cut]);
        super::count_shape();
        let shape = ShapeLine::new(fonts.font_system(), &text, &attrs, Shaping::Advanced, shaped.tab_width);
        (text, shape)
    };
    let (mut lo, mut hi, mut best) = (0usize, chars.len(), 0usize);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let cut = chars.get(mid).copied().unwrap_or(para_text.len());
        let (_, shape) = shape_prefix(cut);
        if break_line(shaped, &shape, params.width, params.wrap, None, scratch).len() <= k {
            best = cut;
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    let (text, shape) = shape_prefix(best);
    ElidedShape { para: i, cut: best, text, shape }
}

/// Width of `line` without its logically-trailing whitespace (pre-swap semantics).
fn content_width(line: &LayoutLine, line_text: &str) -> f32 {
    let total: f32 = line.glyphs.iter().map(|g| g.w).sum();
    let mut glyphs: Vec<&cosmic_text::LayoutGlyph> = line.glyphs.iter().collect();
    glyphs.sort_by_key(|g| std::cmp::Reverse(g.end));
    let mut trailing = 0.0;
    for g in glyphs {
        let cluster = line_text.get(g.start..g.end).unwrap_or("");
        if !cluster.is_empty() && cluster.chars().all(char::is_whitespace) {
            trailing += g.w;
        } else {
            break;
        }
    }
    total - trailing
}

impl TextLayout {
    /// Lay `shaped` out with `params`. `fonts` is needed only to reshape an elided variant.
    pub(crate) fn new(
        shaped: Rc<ShapedText>,
        params: LayoutParams,
        scratch: &mut ShapeBuffer,
        fonts: &mut FontRegistry,
    ) -> TextLayout {
        super::count_layout();
        // Justification only stretches the spaces of already-broken lines, so justified text is
        // measured unjustified (the pre-swap width is the natural advance) and placed justified.
        let justified = params.align == Some(cosmic_text::Align::Justified);
        let measure_align = if justified { None } else { params.align };
        let can_elide = params.ellipsis && !shaped.text.is_empty();
        // Quirk Q-2: max_lines without ellipsis reports the width of every laid-out line.
        let q2 = params.max_lines.is_some() && !params.ellipsis;
        let mut remaining = params.max_lines.map(|m| m as usize);
        // Lines kept per kept paragraph (pass 1 decides; re-breaks reuse the counts).
        let mut keep: Vec<usize> = Vec::new();
        let mut q2_width: f32 = 0.0;
        let mut elided: Option<ElidedShape> = None;
        let n = shaped.paras.len();
        let q2_measure = |from: usize, skip: usize, q2_width: &mut f32, scratch: &mut ShapeBuffer| {
            for (j, para) in shaped.paras.iter().enumerate().skip(from) {
                if para.placeholder {
                    continue;
                }
                let text = &shaped.text[para.range.clone()];
                let lines =
                    break_line(&shaped, &para.shape, params.width, params.wrap, measure_align, scratch);
                for l in lines.iter().skip(if j == from { skip } else { 0 }) {
                    *q2_width = q2_width.max(content_width(l, text));
                }
            }
        };
        // Pass 1 — break lines under the max_lines budget (§3.7.3–§3.7.4).
        for (i, para) in shaped.paras.iter().enumerate() {
            let count =
                break_line(&shaped, &para.shape, params.width, params.wrap, measure_align, scratch).len();
            let Some(left) = remaining else {
                keep.push(count);
                continue;
            };
            if count < left || (count == left && i + 1 == n) {
                keep.push(count);
                remaining = Some(left - count);
                continue;
            }
            if count == left {
                // Budget exhausted exactly at this paragraph's end with more to come.
                keep.push(count);
                if can_elide {
                    elided = Some(elide(&shaped, i, count, &params, scratch, fonts));
                } else if q2 {
                    q2_measure(i + 1, 0, &mut q2_width, scratch);
                }
                break;
            }
            // Overflow inside paragraph i.
            keep.push(left);
            if can_elide {
                elided = Some(elide(&shaped, i, left, &params, scratch, fonts));
            } else if q2 {
                q2_measure(i, left, &mut q2_width, scratch);
            }
            break;
        }
        let kept_paras = keep.len();
        let mut layout = TextLayout {
            lines: Vec::new(),
            elided: None,
            params,
            width: 0.0,
            height: 0.0,
            align_width: 0.0,
            placeholder_boxes: Vec::new(),
            kept_paras,
            shaped,
        };
        let raw =
            layout.break_kept(&keep, elided.as_ref(), params.width, params.wrap, measure_align, scratch);
        layout.place(raw);
        layout.elided = elided.as_ref().map(|e| Elided { para: e.para, cut: e.cut, text: e.text.clone() });
        layout.measure();
        layout.width = layout.width.max(q2_width);
        match params.width {
            Some(w) => {
                layout.align_width = w;
                if justified {
                    let raw = layout.break_kept(
                        &keep,
                        elided.as_ref(),
                        Some(w),
                        params.wrap,
                        params.align,
                        scratch,
                    );
                    layout.place(raw);
                }
            }
            // Pass 2 — unbounded multi-paragraph text aligns every line inside one common width.
            None if kept_paras > 1 => {
                let w = layout.width;
                let raw = layout.break_kept(&keep, elided.as_ref(), Some(w), false, params.align, scratch);
                layout.place(raw);
                layout.align_width = w;
            }
            None => {
                layout.align_width = layout.width;
                if justified {
                    let raw = layout.break_kept(&keep, elided.as_ref(), None, false, params.align, scratch);
                    layout.place(raw);
                }
            }
        }
        layout
    }

    /// Break every kept paragraph (the elided one through its variant), keeping `keep[i]` lines.
    fn break_kept(
        &self,
        keep: &[usize],
        elided: Option<&ElidedShape>,
        width: Option<f32>,
        wrap: bool,
        align: Option<cosmic_text::Align>,
        scratch: &mut ShapeBuffer,
    ) -> Vec<(usize, bool, LayoutLine)> {
        let mut raw = Vec::new();
        for (i, &k) in keep.iter().enumerate() {
            let (shape, from_elided) = match elided {
                Some(e) if e.para == i => (&e.shape, true),
                _ => (&self.shaped.paras[i].shape, false),
            };
            let lines = break_line(&self.shaped, shape, width, wrap, align, scratch);
            raw.extend(lines.into_iter().take(k).map(|l| (i, from_elided, l)));
        }
        raw
    }

    /// Stack `raw` lines with cosmic's per-line vertical metrics (§3.7.2).
    fn place(&mut self, raw: Vec<(usize, bool, LayoutLine)>) {
        let mut top = 0.0;
        let mut boxes: Vec<(usize, kurbo::Rect)> = Vec::new();
        let mut lines = Vec::with_capacity(raw.len());
        for (para, from_elided, layout) in raw {
            let mut height = layout.line_height_opt.unwrap_or(self.shaped.base_line_height_px);
            // Extent above the baseline: half the leading over the glyph box (cosmic-text's
            // centering), then at least the strut's (exactly the strut's when forced).
            let mut above = (height - (layout.max_ascent + layout.max_descent)) / 2.0 + layout.max_ascent;
            if let Some(strut) = self.params.strut {
                let below = height - above;
                let (a, b) = if strut.force {
                    (strut.above, strut.below)
                } else {
                    (above.max(strut.above), below.max(strut.below))
                };
                above = a;
                height = a + b;
            }
            // Inline placeholders grow the line to contain them (T48).
            let inline = self.inline_boxes(para, from_elided, &layout);
            for (_, _, p) in &inline {
                use crate::objects::PlaceholderAlignment as A;
                let below = height - above;
                let h = p.height.max(0.0);
                let (need_above, need_below) = match p.alignment {
                    A::Baseline => (p.baseline_offset, h - p.baseline_offset),
                    A::AboveBaseline => (h, 0.0),
                    A::BelowBaseline => (0.0, h),
                    A::Top => (above, h - above),
                    A::Bottom => (h - below, below),
                    A::Middle => {
                        let grow = (h - height).max(0.0) / 2.0;
                        (above + grow, below + grow)
                    }
                };
                above = above.max(need_above);
                height = above + (height - above).max(need_below).max(below);
            }
            let baseline = top + above;
            for (index, x, p) in inline {
                use crate::objects::PlaceholderAlignment as A;
                let h = p.height.max(0.0);
                let y0 = match p.alignment {
                    A::Baseline => baseline - p.baseline_offset,
                    A::AboveBaseline => baseline - h,
                    A::BelowBaseline => baseline,
                    A::Top => top,
                    A::Bottom => top + height - h,
                    A::Middle => top + (height - h) / 2.0,
                };
                let r = kurbo::Rect::new(
                    f64::from(x),
                    f64::from(y0),
                    f64::from(x + p.width.max(0.0)),
                    f64::from(y0 + h),
                );
                boxes.push((index, r));
            }
            lines.push(VisualLine { para, from_elided, layout, top, height, baseline });
            top += height;
        }
        self.lines = lines;
        boxes.sort_by_key(|(i, _)| *i);
        self.placeholder_boxes = boxes.into_iter().map(|(_, r)| r).collect();
    }

    /// The placeholders on one laid-out line: (placeholder index, x, placeholder).
    fn inline_boxes(
        &self,
        para: usize,
        from_elided: bool,
        layout: &LayoutLine,
    ) -> Vec<(usize, f32, crate::objects::Placeholder)> {
        if self.shaped.placeholders.is_empty() || from_elided {
            return Vec::new();
        }
        let start = self.shaped.paras[para].range.start;
        layout
            .glyphs
            .iter()
            .filter_map(|g| {
                let at = start + g.start;
                let i = self.shaped.placeholders.iter().position(|(b, _)| *b == at)?;
                Some((i, g.x, self.shaped.placeholders[i].1))
            })
            .collect()
    }

    /// Inline placeholder boxes in layout space, in placeholder order (T48).
    pub fn placeholder_boxes(&self) -> &[kurbo::Rect] {
        &self.placeholder_boxes
    }

    /// Width = widest content; height = Σ line heights, including the empty last line of a text
    /// that ends with a separator (the pre-swap build and Flutter both count it, §3.7.6).
    fn measure(&mut self) {
        let mut width: f32 = 0.0;
        for (i, line) in self.lines.iter().enumerate() {
            if line.from_elided || !self.shaped.paras[line.para].placeholder {
                // Whitespace a soft wrap hangs past the line end is not measured, but whitespace
                // ending a paragraph is real advance: Flutter's paragraph (SkParagraph) keeps it
                // in maxIntrinsicWidth, which TextPainter reports as the width, so "Press "
                // leaves its space before the next widget in a row.
                let ends_para = self.lines.get(i + 1).is_none_or(|n| n.para != line.para);
                let w = if ends_para && !line.from_elided {
                    line.layout.glyphs.iter().map(|g| g.w).sum()
                } else {
                    content_width(&line.layout, self.line_text(line))
                };
                width = width.max(w);
            }
        }
        if let Some(max) = self.params.width {
            width = width.min(max.max(0.0));
        }
        self.width = width;
        self.height = self.lines.iter().map(|l| l.height).sum();
    }

    /// (width, height).
    pub fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    pub fn width(&self) -> f32 {
        self.width
    }

    pub fn height(&self) -> f32 {
        self.height
    }

    /// The first line's baseline (layout space).
    pub fn first_baseline(&self) -> Option<f32> {
        self.lines.first().map(|l| l.baseline)
    }

    /// Visual lines, including a trailing empty line.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn text(&self) -> &str {
        &self.shaped.text
    }

    pub fn is_elided(&self) -> bool {
        self.elided.is_some()
    }

    /// Every visual line's top (test seam for the text pin, T10).
    #[doc(hidden)]
    pub fn line_tops(&self) -> Vec<f32> {
        self.lines.iter().map(|l| l.top).collect()
    }

    #[allow(dead_code)] // paint and hit-testing walk lines from T18.
    pub(crate) fn lines(&self) -> &[VisualLine] {
        &self.lines
    }

    #[allow(dead_code)]
    pub(crate) fn paras(&self) -> &[ShapedPara] {
        &self.shaped.paras
    }

    /// The text a line's glyph byte ranges index: the paragraph, the elided variant, or the
    /// placeholder space an empty paragraph is shaped as.
    pub(crate) fn line_text(&self, line: &VisualLine) -> &str {
        let para = &self.shaped.paras[line.para];
        match (&self.elided, line.from_elided) {
            (Some(e), true) => &e.text,
            _ if para.placeholder => " ",
            _ => &self.shaped.text[para.range.clone()],
        }
    }
}

/// (min, max) intrinsic widths of `shaped` (§3.7.7), from its ONE unbounded layout — no
/// reshape. max = that layout's width; min = the widest run between spaces / NBSPs / line
/// breaks (Flutter's `getMinIntrinsicWidth` notion, pre-swap parity), or max when that is 0.
/// Memoized in `ShapedText::intrinsic`.
#[allow(dead_code)] // RenderParagraph::intrinsic reads it from T22.
pub(crate) fn intrinsic_widths(
    shaped: &Rc<ShapedText>,
    scratch: &mut ShapeBuffer,
    fonts: &mut FontRegistry,
) -> (f32, f32) {
    if let Some(memo) = shaped.intrinsic.get() {
        return memo;
    }
    let params = LayoutParams {
        width: None,
        wrap: false,
        align: Some(cosmic_text::Align::Left),
        max_lines: None,
        ellipsis: false,
        strut: None,
    };
    let layout = TextLayout::new(shaped.clone(), params, scratch, fonts);
    let (mut min, mut cur) = (0.0_f32, 0.0_f32);
    for line in &layout.lines {
        let text = layout.line_text(line);
        for g in &line.layout.glyphs {
            let cluster = text.get(g.start..g.end).unwrap_or("");
            if !cluster.is_empty() && cluster.chars().all(|c| c == ' ' || c == '\u{a0}') {
                min = min.max(cur);
                cur = 0.0;
            } else {
                cur += g.w;
            }
        }
        // A line break is a word boundary too.
        min = min.max(cur);
        cur = 0.0;
    }
    let max = layout.width;
    let pair = (if min > 0.0 { min } else { max }, max);
    shaped.intrinsic.set(Some(pair));
    pair
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::ParagraphStyle;
    use crate::text::{TextEnv, layout_for_test};
    use pebbles_foundation::TextAlign;

    fn style(family: &str, size: f32) -> ParagraphStyle {
        ParagraphStyle { font_family: Some(family.into()), font_size: size, ..ParagraphStyle::default() }
    }

    fn mono() -> ParagraphStyle {
        style("JetBrains Mono", 20.0)
    }

    fn lay(text: &str, s: &ParagraphStyle, width: Option<f32>) -> Rc<TextLayout> {
        layout_for_test(&mut TextEnv::new(), text, s, &[], width)
    }

    fn near(a: f32, b: f32, tol: f32) -> bool {
        (a - b).abs() <= tol
    }

    /// Pen x of the first glyph of visual line `i`.
    fn pen_x(l: &TextLayout, i: usize) -> f32 {
        let g = &l.lines[i].layout.glyphs[0];
        g.x + g.font_size * g.x_offset
    }

    #[test]
    fn inter16_hello_metrics() {
        let l = lay("Hello", &style("Inter", 16.0), None);
        assert!(near(l.height(), 19.2, 0.001), "{}", l.height());
        assert!(near(l.first_baseline().unwrap(), 15.4203, 0.01), "{:?}", l.first_baseline());
        assert_eq!(l.line_count(), 1);
    }

    #[test]
    fn mono20_width_height_baseline() {
        let l = lay("abcdefghij", &mono(), None);
        assert!(near(l.width(), 120.0, 0.01), "{}", l.width());
        assert!(near(l.height(), 24.0, 0.001));
        assert!(near(l.first_baseline().unwrap(), 19.2, 0.01));
    }

    #[test]
    fn mono20_letter_spacing() {
        let l = lay("abcd", &ParagraphStyle { letter_spacing: 2.0, ..mono() }, None);
        assert!(near(l.width(), 56.0, 0.01), "{}", l.width());
    }

    #[test]
    fn trailing_whitespace_kept_at_paragraph_end() {
        // Regression: "Press " dropped its space and ran into the next widget. Flutter keeps a
        // paragraph's trailing whitespace in its (intrinsic) width.
        assert!(near(lay("abc   ", &mono(), None).width(), 72.0, 0.01));
        assert!(near(lay("ab \ncd", &mono(), None).width(), 36.0, 0.01), "hard-break line keeps it");
    }

    #[test]
    fn whitespace_hanging_at_a_soft_wrap_is_not_measured() {
        // "abc def" wrapped at 50 px: line 1 is "abc " whose space hangs — width stays 36.
        let l = lay("abc def", &mono(), Some(50.0));
        assert_eq!(l.line_count(), 2);
        assert!(near(l.width(), 36.0, 0.01), "{}", l.width());
    }

    #[test]
    fn trailing_newline_height() {
        // Three lines of 19.2: the empty last line counts (oracle pin 57.6; Flutter alike).
        let l = lay("line1\nline2\n", &style("Inter", 16.0), None);
        assert!(near(l.height(), 57.6, 0.001), "{}", l.height());
        assert_eq!(l.line_count(), 3);
    }

    #[test]
    fn wrap_word_mono() {
        let l = lay("aaaa bbbb cccc", &mono(), Some(100.0));
        assert_eq!(l.line_count(), 3);
        assert!(near(l.width(), 48.0, 0.01), "{}", l.width());
        assert!(near(l.height(), 72.0, 0.001));
    }

    #[test]
    fn long_word_overflows() {
        let l = lay("aaaaaaaaaaaaaaaa bb", &mono(), Some(100.0));
        assert_eq!(l.line_count(), 2);
        let w0 = content_width(&l.lines[0].layout, l.line_text(&l.lines[0]));
        assert!(near(w0, 192.0, 0.01), "{w0}");
    }

    #[test]
    fn center_unbounded_two_paras() {
        let center =
            lay("ab\nabcd", &ParagraphStyle { align: TextAlign::Center, soft_wrap: false, ..mono() }, None);
        assert!(near(pen_x(&center, 0), 12.0, 0.01), "{}", pen_x(&center, 0));
        assert!(near(pen_x(&center, 1), 0.0, 0.01));
        let right =
            lay("ab\nabcd", &ParagraphStyle { align: TextAlign::Right, soft_wrap: false, ..mono() }, None);
        assert!(near(pen_x(&right, 0), 24.0, 0.01), "{}", pen_x(&right, 0));
    }

    #[test]
    fn justify_mono() {
        let l = lay("aa bb cc", &ParagraphStyle { align: TextAlign::Justify, ..mono() }, Some(84.0));
        assert!(near(l.width(), 60.0, 0.01), "measured unjustified: {}", l.width());
        let b = l.lines[0].layout.glyphs.iter().find(|g| g.start == 3).expect("first b");
        assert!(near(b.x + b.font_size * b.x_offset, 60.0, 0.01), "{}", b.x);
        assert!(near(pen_x(&l, 1), 0.0, 0.01));
    }

    #[test]
    fn empty_text() {
        let l = lay("", &style("Inter", 16.0), None);
        assert_eq!(l.width(), 0.0);
        assert!(near(l.height(), 19.2, 0.001));
        assert_eq!(l.line_count(), 1);
        assert!(near(l.first_baseline().unwrap(), 15.4203, 0.25));
    }

    #[test]
    fn crlf_paras() {
        assert_eq!(lay("a\r\nb", &style("Lora", 16.0), None).line_count(), 2);
    }

    fn clamp(s: ParagraphStyle, max_lines: u32, ellipsis: bool) -> ParagraphStyle {
        ParagraphStyle { max_lines: Some(max_lines), ellipsis, ..s }
    }

    #[test]
    fn ellipsis_inside_paragraph() {
        let l = lay("aaaa bbbb cccc", &clamp(mono(), 2, true), Some(100.0));
        assert_eq!(l.line_count(), 2);
        assert!(l.is_elided());
        assert!(near(l.height(), 48.0, 0.001));
        let e = l.elided.as_ref().expect("elided");
        assert_eq!(e.cut, 12);
        assert_eq!(e.text, "aaaa bbbb cc\u{2026}");
        let w1 = content_width(&l.lines[1].layout, l.line_text(&l.lines[1]));
        assert!(near(w1, 96.0, 0.01), "{w1}");
    }

    #[test]
    fn ellipsis_at_paragraph_boundary() {
        let l = lay("one\ntwo\nthree", &clamp(ParagraphStyle { soft_wrap: false, ..mono() }, 2, true), None);
        assert_eq!(l.line_count(), 2);
        assert_eq!(l.elided.as_ref().map(|e| e.text.as_str()), Some("two\u{2026}"));
        assert!(near(l.width(), 48.0, 0.01), "{}", l.width());
        assert!(near(l.height(), 48.0, 0.001));
    }

    #[test]
    fn max_lines_without_ellipsis_keeps_quirk_q2() {
        let l = lay("a\nbbbbbbbb\nc", &clamp(ParagraphStyle { soft_wrap: false, ..mono() }, 1, false), None);
        assert_eq!(l.line_count(), 1);
        assert!(near(l.height(), 24.0, 0.001));
        assert!(near(l.width(), 96.0, 0.01), "{}", l.width());
    }

    #[test]
    fn trailing_newline_ellipsis() {
        let l = lay("a\nb\n", &clamp(ParagraphStyle { soft_wrap: false, ..mono() }, 2, true), None);
        assert_eq!(l.elided.as_ref().map(|e| e.text.as_str()), Some("b\u{2026}"));
    }

    #[test]
    fn empty_text_never_elides() {
        assert!(!lay("", &clamp(mono(), 1, true), None).is_elided());
    }

    fn shaped(text: &str, size: f32) -> (Rc<ShapedText>, TextEnv) {
        let mut env = TextEnv::new();
        let style = ParagraphStyle { font_size: size, ..mono() };
        let spec = crate::text::style::TextStyleSpec::from(&style);
        let shaped = Rc::new(crate::text::shape::shape_text(&mut env.registry, text, &spec, &[]));
        (shaped, env)
    }

    #[test]
    fn intrinsic_mono() {
        let (s, mut env) = shaped("aaa bbbbbb cc", 10.0);
        let (min, max) = intrinsic_widths(&s, &mut env.scratch, &mut env.registry);
        assert!(near(min, 36.0, 0.01) && near(max, 78.0, 0.01), "{min} {max}");
    }

    #[test]
    fn intrinsic_nbsp_is_a_break() {
        let (s, mut env) = shaped("aa\u{a0}bbbb", 10.0);
        let (min, _) = intrinsic_widths(&s, &mut env.scratch, &mut env.registry);
        assert!(near(min, 24.0, 0.01), "{min}");
    }

    #[test]
    #[cfg(debug_assertions)] // the shape/layout counters are debug-only
    fn intrinsic_does_not_reshape() {
        let (s, mut env) = shaped("aaa bbbbbb cc", 10.0);
        let (shapes, layouts) = (crate::text::shape_count(), crate::text::layout_count());
        let first = intrinsic_widths(&s, &mut env.scratch, &mut env.registry);
        for _ in 0..9 {
            assert_eq!(intrinsic_widths(&s, &mut env.scratch, &mut env.registry), first);
        }
        assert_eq!(crate::text::shape_count(), shapes, "no reshape");
        assert_eq!(crate::text::layout_count() - layouts, 1, "the walk runs once");
    }
}

#[cfg(test)]
mod placeholder_tests {
    use crate::objects::{ParagraphStyle, Placeholder, PlaceholderAlignment};
    use crate::text::style::TextStyleSpec;
    use crate::text::{Affinity, TextEnv, TextLayout, TextRequest};
    use std::rc::Rc;

    fn ph(width: f32, height: f32, alignment: PlaceholderAlignment, baseline_offset: f32) -> Placeholder {
        Placeholder { width, height, alignment, baseline_offset }
    }

    /// JetBrains Mono 20: a 24 px line with its baseline at 19.2.
    fn lay(text: &str, placeholders: Vec<Placeholder>, width: Option<f32>) -> Rc<TextLayout> {
        let mut env = TextEnv::new();
        let style = ParagraphStyle {
            font_family: Some("JetBrains Mono".into()),
            font_size: 20.0,
            ..ParagraphStyle::default()
        };
        let mut spec = TextStyleSpec::from(&style);
        spec.placeholders = placeholders;
        let params = super::LayoutParams {
            width,
            wrap: width.is_some(),
            align: Some(cosmic_text::Align::Left),
            max_lines: None,
            ellipsis: false,
            strut: None,
        };
        let req = TextRequest { text, style: &spec, spans: &[], params };
        let key = env.text_key(&req);
        env.layout_text(&req, key)
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() <= 0.01
    }

    fn one(alignment: PlaceholderAlignment, height: f32, offset: f32) -> (kurbo::Rect, f32) {
        let l = lay("a\u{FFFC}b", vec![ph(30.0, height, alignment, offset)], None);
        (l.placeholder_boxes()[0], l.height())
    }

    #[test]
    fn baseline_alignment() {
        let (r, h) = one(PlaceholderAlignment::Baseline, 30.0, 25.0);
        assert!(near(r.x0, 12.0) && near(r.x1, 42.0) && near(r.y0, 0.0) && near(r.y1, 30.0), "{r:?}");
        assert!((h - 30.0).abs() < 0.01, "{h}");
    }

    #[test]
    fn above_baseline_alignment() {
        let (r, h) = one(PlaceholderAlignment::AboveBaseline, 10.0, 0.0);
        assert!(near(r.y0, 9.2) && near(r.y1, 19.2), "{r:?}");
        assert!((h - 24.0).abs() < 0.01);
    }

    #[test]
    fn below_baseline_alignment() {
        let (r, h) = one(PlaceholderAlignment::BelowBaseline, 10.0, 0.0);
        assert!(near(r.y0, 19.2) && near(r.y1, 29.2), "{r:?}");
        assert!((h - 29.2).abs() < 0.01, "{h}");
    }

    #[test]
    fn top_alignment() {
        let (r, _) = one(PlaceholderAlignment::Top, 10.0, 0.0);
        assert!(near(r.y0, 0.0), "{r:?}");
    }

    #[test]
    fn bottom_alignment() {
        let (r, _) = one(PlaceholderAlignment::Bottom, 10.0, 0.0);
        assert!(near(r.y0, 14.0), "{r:?}");
    }

    #[test]
    fn middle_alignment() {
        let (r, h) = one(PlaceholderAlignment::Middle, 40.0, 0.0);
        assert!((h - 40.0).abs() < 0.01 && near(r.y0, 0.0), "{r:?} {h}");
    }

    #[test]
    fn placeholder_wraps_like_a_word() {
        let l =
            lay("aaaa \u{FFFC} bbbb", vec![ph(80.0, 10.0, PlaceholderAlignment::Bottom, 0.0)], Some(100.0));
        assert_eq!(l.line_count(), 3, "{:?}", l.placeholder_boxes());
        assert!(near(l.placeholder_boxes()[0].x0, 0.0), "the box starts its own line");
    }

    #[test]
    fn caret_steps_over_placeholder_once() {
        let l = lay("a\u{FFFC}b", vec![ph(30.0, 10.0, PlaceholderAlignment::Bottom, 0.0)], None);
        assert_eq!(crate::text::motion::right(&l, 1, 1, false).1, 4);
        assert_eq!(crate::text::motion::left(&l, 4, 4, false).1, 1);
    }

    #[test]
    fn hit_inside_placeholder_returns_its_boundary() {
        let l = lay("a\u{FFFC}b", vec![ph(30.0, 10.0, PlaceholderAlignment::Bottom, 0.0)], None);
        assert_eq!(l.hit(15.0, 10.0), (1, Affinity::Downstream));
        assert_eq!(l.hit(38.0, 10.0), (4, Affinity::Upstream));
    }
}

#[cfg(test)]
mod strut_tests {
    use crate::objects::{ParagraphStyle, StrutStyle, TextSpanStyle};
    use crate::text::{TextEnv, layout_for_test};

    fn strut(size: f32, force: bool) -> StrutStyle {
        StrutStyle {
            font_family: None,
            font_size: size,
            height: None,
            leading: None,
            force_strut_height: force,
        }
    }

    #[test]
    fn strut_sets_minimum_line_height() {
        let plain = layout_for_test(&mut TextEnv::new(), "Hello", &ParagraphStyle::default(), &[], None);
        let style = ParagraphStyle { strut: Some(strut(32.0, false)), ..ParagraphStyle::default() };
        let l = layout_for_test(&mut TextEnv::new(), "Hello", &style, &[], None);
        // Inter at 32 px: ascent + descent = (1984 + 494) / 2048 × 32.
        let strut_h = (1984.0 + 494.0) / 2048.0 * 32.0;
        assert!((l.height() - strut_h).abs() < 0.01, "{} vs {strut_h}", l.height());
        assert!(l.height() > plain.height());
        assert!(l.first_baseline().unwrap() > plain.first_baseline().unwrap());
    }

    #[test]
    fn forced_strut_ignores_tall_glyphs() {
        let mut big = TextSpanStyle::new(0..5);
        big.size = Some(48.0);
        let style = ParagraphStyle { strut: Some(strut(16.0, true)), ..ParagraphStyle::default() };
        let l = layout_for_test(&mut TextEnv::new(), "Hello", &style, &[big], None);
        let strut_h = (1984.0 + 494.0) / 2048.0 * 16.0;
        assert!((l.height() - strut_h).abs() < 0.01, "{} vs {strut_h}", l.height());
    }

    #[test]
    fn strut_height_and_leading() {
        let s = StrutStyle { height: Some(2.0), leading: Some(0.5), ..strut(20.0, true) };
        let style = ParagraphStyle { strut: Some(s), ..ParagraphStyle::default() };
        let l = layout_for_test(&mut TextEnv::new(), "Hi", &style, &[], None);
        assert!((l.height() - (40.0 + 10.0)).abs() < 0.01, "{}", l.height());
    }
}

#[cfg(test)]
mod robustness {
    use crate::objects::ParagraphStyle;
    use crate::text::{TextEnv, layout_for_test};
    use pebbles_foundation::TextAlign;

    fn next(state: &mut u64) -> u64 {
        // xorshift64
        let mut x = *state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        *state = x;
        x
    }

    fn pick<'a, T>(state: &mut u64, items: &'a [T]) -> &'a T {
        &items[(next(state) % items.len() as u64) as usize]
    }

    const ALPHABET: &[&str] = &[
        "a",
        "Z",
        "5",
        " ",
        "\t",
        "\n",
        "\r",
        "\r\n",
        "\u{85}",
        "\u{2029}",
        "\u{1E}",
        "\u{a0}",
        "é",
        "e\u{301}",
        "\u{5d0}",
        "\u{627}",
        "\u{4e2d}",
        "🙂",
        "👩\u{200d}👩\u{200d}👧",
        "\u{202E}",
        "\u{202C}",
        "\u{2066}",
        "\u{2069}",
        "\u{0}",
        "\u{FFFD}",
    ];
    const ALIGNS: &[TextAlign] = &[
        TextAlign::Left,
        TextAlign::Right,
        TextAlign::Center,
        TextAlign::Justify,
        TextAlign::Start,
        TextAlign::End,
    ];

    #[test]
    fn seeded_layout_never_panics() {
        let mut env = TextEnv::new();
        for seed in [1u64, 2, 3, 0xDEAD_BEEF] {
            let mut state = seed;
            for _ in 0..500 {
                let tokens = next(&mut state) % 200;
                let text: String = (0..tokens).map(|_| *pick(&mut state, ALPHABET)).collect();
                let style = ParagraphStyle {
                    font_size: *pick(&mut state, &[0.0, f32::NAN, 1.0, 13.0, 16.0, 1e9]),
                    max_lines: *pick(&mut state, &[None, Some(0), Some(1), Some(3)]),
                    ellipsis: next(&mut state).is_multiple_of(2),
                    align: *pick(&mut state, ALIGNS),
                    tab_width: *pick(&mut state, &[0, 1, 4]),
                    ..ParagraphStyle::default()
                };
                let width =
                    *pick(&mut state, &[None, Some(0.0), Some(1.0), Some(57.5), Some(f32::NAN), Some(1e7)]);
                let l = layout_for_test(&mut env, &text, &style, &[], width);
                let ctx = format!("seed {seed:#x} text {text:?} style {style:?} width {width:?}");
                assert!(l.width().is_finite() && l.width() >= 0.0, "width {} — {ctx}", l.width());
                assert!(l.height().is_finite() && l.height() >= 0.0, "height {} — {ctx}", l.height());
                let tops = l.line_tops();
                assert!(tops.windows(2).all(|w| w[0] <= w[1]), "tops {tops:?} — {ctx}");
                for line in &l.lines {
                    let t = l.line_text(line);
                    for g in &line.layout.glyphs {
                        assert!(
                            t.is_char_boundary(g.start) && t.is_char_boundary(g.end),
                            "glyph {}..{} off a char boundary of {t:?} — {ctx}",
                            g.start,
                            g.end
                        );
                    }
                }
            }
        }
    }
}
