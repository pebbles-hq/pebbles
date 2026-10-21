//! Laid-out text (03 §3.7): line breaking per paragraph through cosmic-text, cosmic's vertical
//! metrics, Parley-parity measurement (trailing whitespace excluded, justified text measured
//! unjustified, the empty last line of a text ending with a separator counted) and a common
//! alignment width for unbounded multi-paragraph text.

use std::rc::Rc;

use cosmic_text::{Ellipsize, Hinting, LayoutLine, ShapeBuffer, Wrap};

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

#[allow(dead_code)] // `para`/`cut` drive caret mapping and paint of the elided line (T18+).
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
    /// `paras[kept_paras..]` were dropped by max_lines.
    pub(crate) kept_paras: usize,
}

/// Break one paragraph into layout lines.
fn break_para(
    shaped: &ShapedText,
    para: &ShapedPara,
    width: Option<f32>,
    wrap: bool,
    align: Option<cosmic_text::Align>,
    scratch: &mut ShapeBuffer,
) -> Vec<LayoutLine> {
    let mut out = Vec::new();
    para.shape.layout_to_buffer(
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

/// Width of `line` without its logically-trailing whitespace (Parley semantics).
fn content_width(line: &VisualLine, line_text: &str) -> f32 {
    let total: f32 = line.layout.glyphs.iter().map(|g| g.w).sum();
    let mut glyphs: Vec<&cosmic_text::LayoutGlyph> = line.layout.glyphs.iter().collect();
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
        _fonts: &mut FontRegistry,
    ) -> TextLayout {
        super::count_layout();
        // Pass 1 — break lines (max_lines / ellipsis: T13). Justification only stretches the
        // spaces of already-broken lines, so justified text is measured unjustified (Parley's
        // width is the natural advance) and placed justified.
        let justified = params.align == Some(cosmic_text::Align::Justified);
        let measure_align = if justified { None } else { params.align };
        let mut raw: Vec<(usize, LayoutLine)> = Vec::new();
        for (i, para) in shaped.paras.iter().enumerate() {
            for line in break_para(&shaped, para, params.width, params.wrap, measure_align, scratch) {
                raw.push((i, line));
            }
        }
        let kept_paras = shaped.paras.len();
        let mut layout = TextLayout {
            lines: Vec::new(),
            elided: None,
            params,
            width: 0.0,
            height: 0.0,
            align_width: 0.0,
            kept_paras,
            shaped,
        };
        layout.place(raw);
        layout.measure();
        if justified {
            let mut raw = Vec::new();
            for (i, para) in layout.shaped.paras[..layout.kept_paras].iter().enumerate() {
                for line in break_para(&layout.shaped, para, params.width, params.wrap, params.align, scratch)
                {
                    raw.push((i, line));
                }
            }
            layout.place(raw);
        }
        // Pass 2 — unbounded multi-paragraph text aligns every line inside one common width.
        match params.width {
            Some(w) => layout.align_width = w,
            None if layout.kept_paras > 1 => {
                let w = layout.width;
                let mut raw = Vec::new();
                for (i, para) in layout.shaped.paras[..layout.kept_paras].iter().enumerate() {
                    for line in break_para(&layout.shaped, para, Some(w), false, params.align, scratch) {
                        raw.push((i, line));
                    }
                }
                layout.place(raw);
                layout.align_width = w;
            }
            None => layout.align_width = layout.width,
        }
        layout
    }

    /// Stack `raw` lines with cosmic's per-line vertical metrics (§3.7.2).
    fn place(&mut self, raw: Vec<(usize, LayoutLine)>) {
        let mut top = 0.0;
        self.lines = raw
            .into_iter()
            .map(|(para, layout)| {
                let height = layout.line_height_opt.unwrap_or(self.shaped.base_line_height_px);
                let baseline =
                    top + (height - (layout.max_ascent + layout.max_descent)) / 2.0 + layout.max_ascent;
                let line = VisualLine { para, from_elided: false, layout, top, height, baseline };
                top += height;
                line
            })
            .collect();
    }

    /// Width = widest content; height = Σ line heights, including the empty last line of a text
    /// that ends with a separator (Parley and Flutter both count it, §3.7.6).
    fn measure(&mut self) {
        let mut width: f32 = 0.0;
        for line in &self.lines {
            if !self.shaped.paras[line.para].placeholder {
                width = width.max(content_width(line, self.line_text(line)));
            }
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

    /// The text a line's glyph byte ranges index (the paragraph, or the elided variant).
    pub(crate) fn line_text(&self, line: &VisualLine) -> &str {
        match (&self.elided, line.from_elided) {
            (Some(e), true) => &e.text,
            _ => &self.shaped.text[self.shaped.paras[line.para].range.clone()],
        }
    }
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
    fn trailing_whitespace_excluded() {
        assert!(near(lay("abc   ", &mono(), None).width(), 36.0, 0.01));
    }

    #[test]
    fn trailing_newline_height() {
        // Three lines of 19.2: the empty last line counts (Parley pin 57.6; Flutter alike).
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
        let w0 = content_width(&l.lines[0], l.line_text(&l.lines[0]));
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
}
