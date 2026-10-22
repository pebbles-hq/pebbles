//! Editing geometry over a [`TextLayout`] (03b §3): byte ↔ position mapping with affinity,
//! caret and selection rects, hit-testing and word selection. Pebbles-owned (no
//! `cosmic_text::Editor`); cosmic's per-run `cursor_position` / `highlight` are reused through
//! one `LayoutRun` per visual line.

use std::ops::Range;

use cosmic_text::{Cursor, LayoutRun};
use kurbo::Rect;
use unicode_segmentation::UnicodeSegmentation;

use super::layout::TextLayout;

/// Which side of a boundary a byte position attaches to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Affinity {
    /// Attaches to the previous character. At a soft-wrap boundary: the END of the earlier visual line.
    Upstream,
    /// Attaches to the following character. At a soft-wrap boundary: the START of the next visual line.
    #[default]
    Downstream,
}

impl Affinity {
    /// cosmic-text's equivalent (`Upstream` = `Before`, `Downstream` = `After`).
    pub(crate) fn to_cosmic(self) -> cosmic_text::Affinity {
        match self {
            Affinity::Upstream => cosmic_text::Affinity::Before,
            Affinity::Downstream => cosmic_text::Affinity::After,
        }
    }
}

/// `byte` clamped to `text.len()` and snapped down to a char boundary.
pub(crate) fn snap(text: &str, byte: usize) -> usize {
    let mut b = byte.min(text.len());
    while !text.is_char_boundary(b) {
        b -= 1;
    }
    b
}

impl TextLayout {
    /// (paragraph index, byte offset inside it) of a text byte (clamped and snapped).
    pub(crate) fn para_of(&self, byte: usize) -> (usize, usize) {
        let byte = snap(&self.shaped.text, byte);
        let paras = &self.shaped.paras;
        let p = paras.partition_point(|p| p.range.start <= byte).saturating_sub(1);
        (p, (byte - paras[p].range.start).min(paras[p].range.len()))
    }

    /// Byte span `[min start, max end]` of a line's glyphs (in its line text).
    fn line_span(&self, i: usize) -> Option<(usize, usize)> {
        let glyphs = &self.lines[i].layout.glyphs;
        let start = glyphs.iter().map(|g| g.start).min()?;
        let end = glyphs.iter().map(|g| g.end).max()?;
        Some((start, end))
    }

    /// Whether line `i` addresses no text (an empty paragraph's placeholder or no glyphs).
    fn line_is_empty(&self, i: usize) -> bool {
        let line = &self.lines[i];
        (!line.from_elided && self.shaped.paras[line.para].placeholder) || line.layout.glyphs.is_empty()
    }

    /// The visual line holding `byte` (`aff` picks the side of a soft-wrap boundary). Bytes in
    /// paragraphs dropped by `max_lines`, or past an elided cut, map to the last kept line.
    pub(crate) fn line_for(&self, byte: usize, aff: Affinity) -> usize {
        let (p, local) = self.para_of(byte);
        let lines: Vec<usize> = (0..self.lines.len()).filter(|&i| self.lines[i].para == p).collect();
        let Some(&last) = lines.last() else { return self.lines.len().saturating_sub(1) };
        if lines.len() == 1 || local >= self.shaped.paras[p].range.len() {
            return last;
        }
        // The last line starting at or before `local`; Upstream at a line's exact start belongs to
        // the end of the previous line (the soft-wrap boundary).
        let k = lines.iter().rposition(|&i| self.line_span(i).is_some_and(|(s, _)| s <= local)).unwrap_or(0);
        let starts_here = self.line_span(lines[k]).is_some_and(|(s, _)| s == local);
        if aff == Affinity::Upstream && starts_here && k > 0 { lines[k - 1] } else { lines[k] }
    }

    /// cosmic's view of visual line `i` (so its cursor_position / highlight apply as-is).
    pub(crate) fn run(&self, i: usize) -> LayoutRun<'_> {
        let line = &self.lines[i];
        LayoutRun {
            line_i: line.para,
            text: self.line_text(line),
            rtl: self.shaped.paras[line.para].shape.rtl,
            glyphs: &line.layout.glyphs,
            decorations: &line.layout.decorations,
            line_y: line.baseline,
            line_top: line.top,
            line_height: line.height,
            line_w: line.layout.w,
        }
    }

    /// x of the alignment origin of an empty line of paragraph `p`.
    fn empty_line_x(&self, p: usize) -> f32 {
        let w = self.align_width;
        match self.params.align {
            Some(cosmic_text::Align::Right) => w,
            Some(cosmic_text::Align::Center) => w / 2.0,
            Some(cosmic_text::Align::End) => {
                if self.shaped.paras[p].shape.rtl {
                    0.0
                } else {
                    w
                }
            }
            Some(cosmic_text::Align::Left) | Some(cosmic_text::Align::Justified) => 0.0,
            None => {
                if self.shaped.paras[p].shape.rtl {
                    w
                } else {
                    0.0
                }
            }
        }
    }

    /// The caret at `byte` (`width` wide, the full line height), in layout space.
    pub fn caret_rect(&self, byte: usize, aff: Affinity, width: f32) -> Rect {
        let Some(_) = self.lines.first() else { return Rect::new(0.0, 0.0, f64::from(width), 0.0) };
        let i = self.line_for(byte, aff);
        let line = &self.lines[i];
        let (p, local) = self.para_of(byte);
        let x = if self.line_is_empty(i) {
            self.empty_line_x(line.para)
        } else {
            let run = self.run(i);
            let local = if line.para == p { local } else { run.text.len() };
            run.cursor_position(&Cursor::new_with_affinity(line.para, local, aff.to_cosmic())).unwrap_or_else(
                || {
                    let glyphs = &line.layout.glyphs;
                    if run.rtl {
                        glyphs.iter().map(|g| g.x + g.w).fold(0.0, f32::max)
                    } else {
                        glyphs.iter().map(|g| g.x).fold(f32::INFINITY, f32::min)
                    }
                },
            )
        };
        let (x, top) = (f64::from(x), f64::from(line.top));
        Rect::new(x, top, x + f64::from(width), top + f64::from(line.height))
    }

    /// Highlight rects of the selection `a..b` (either order): several per line for mixed bidi;
    /// a fully selected empty paragraph gets a 6 px stub.
    pub fn selection_rects(&self, a: usize, b: usize, out: &mut Vec<Rect>) {
        let text = &self.shaped.text;
        let (s, e) = (snap(text, a.min(b)), snap(text, a.max(b)));
        if s == e {
            return;
        }
        let ((ps, ls), (pe, le)) = (self.para_of(s), self.para_of(e));
        let start = Cursor::new_with_affinity(ps, ls, cosmic_text::Affinity::After);
        let end = Cursor::new_with_affinity(pe, le, cosmic_text::Affinity::Before);
        for i in 0..self.lines.len() {
            let line = &self.lines[i];
            if line.para < ps || line.para > pe {
                continue;
            }
            let (top, bottom) = (f64::from(line.top), f64::from(line.top + line.height));
            if self.line_is_empty(i) {
                let para = &self.shaped.paras[line.para];
                if s <= para.range.start && e > para.range.start {
                    let x = f64::from(self.empty_line_x(line.para));
                    out.push(Rect::new(x, top, x + 6.0, bottom));
                }
                continue;
            }
            for (x, w) in self.run(i).highlight(start, end) {
                out.push(Rect::new(f64::from(x), top, f64::from(x + w), bottom));
            }
        }
    }

    /// Boxes covering `range` (chips, links): the selection rects of that range.
    pub fn range_boxes(&self, range: Range<usize>, out: &mut Vec<Rect>) {
        self.selection_rects(range.start, range.end, out);
    }

    /// The byte (and affinity) under the point (x, y) in layout space.
    pub fn hit(&self, x: f32, y: f32) -> (usize, Affinity) {
        let text_len = self.shaped.text.len();
        let Some(last) = self.lines.last() else { return (0, Affinity::Downstream) };
        if y >= last.top + last.height {
            return (text_len, Affinity::Upstream);
        }
        let i = self.lines.iter().position(|l| y < l.top + l.height).unwrap_or(0);
        let line = &self.lines[i];
        let para = &self.shaped.paras[line.para];
        let to_text = |local: usize| {
            let local = match (&self.elided, line.from_elided) {
                (Some(e), true) => local.min(e.cut),
                _ => local,
            };
            para.range.start + local.min(para.range.len())
        };
        if self.line_is_empty(i) {
            return (para.range.start, Affinity::Downstream);
        }
        let run = self.run(i);
        let glyphs = &line.layout.glyphs;
        let (span_start, span_end) = self.line_span(i).unwrap_or((0, 0));
        let min_x = glyphs.iter().map(|g| g.x).fold(f32::INFINITY, f32::min);
        let max_x = glyphs.iter().map(|g| g.x + g.w).fold(f32::NEG_INFINITY, f32::max);
        // Outside the glyphs: the visual start / end of the line.
        if x < min_x {
            return if run.rtl {
                (to_text(span_end), Affinity::Upstream)
            } else {
                (to_text(span_start), Affinity::Downstream)
            };
        }
        if x > max_x {
            return if run.rtl {
                (to_text(span_start), Affinity::Downstream)
            } else {
                (to_text(span_end), Affinity::Upstream)
            };
        }
        // Port of cosmic `Buffer::hit`'s per-glyph loop: graphemes split a cluster evenly.
        for g in glyphs {
            if x < g.x || x > g.x + g.w {
                continue;
            }
            let cluster = &run.text[g.start..g.end];
            let total = cluster.grapheme_indices(true).count().max(1);
            let egc_w = g.w / total as f32;
            let mut egc_x = g.x;
            for (k, egc) in cluster.grapheme_indices(true) {
                if x >= egc_x && x <= egc_x + egc_w {
                    let right_half = x >= egc_x + egc_w / 2.0;
                    // The visually later half of a grapheme is past it in reading order.
                    let past = right_half != g.level.is_rtl();
                    return if past {
                        (to_text(g.start + k + egc.len()), Affinity::Upstream)
                    } else {
                        (to_text(g.start + k), Affinity::Downstream)
                    };
                }
                egc_x += egc_w;
            }
            let past = (x >= g.x + g.w / 2.0) != g.level.is_rtl();
            return if past {
                (to_text(g.end), Affinity::Upstream)
            } else {
                (to_text(g.start), Affinity::Downstream)
            };
        }
        // Between glyphs (justified gaps): the nearest glyph edge.
        let nearest = glyphs
            .iter()
            .min_by(|a, b| {
                let d = |g: &&cosmic_text::LayoutGlyph| (g.x - x).abs().min((g.x + g.w - x).abs());
                d(a).total_cmp(&d(b))
            })
            .expect("non-empty line");
        let left_edge_closer = (x - nearest.x).abs() <= (nearest.x + nearest.w - x).abs();
        if left_edge_closer != nearest.level.is_rtl() {
            (to_text(nearest.start), Affinity::Downstream)
        } else {
            (to_text(nearest.end), Affinity::Upstream)
        }
    }

    /// The UAX #29 word under the point (double-click selection): whitespace under the point
    /// selects the word before it when there is one.
    pub fn word_at(&self, x: f32, y: f32) -> (usize, usize) {
        let (byte, _) = self.hit(x, y);
        let (p, local) = self.para_of(byte);
        let para = &self.shaped.paras[p];
        let text = &self.shaped.text[para.range.clone()];
        let segments: Vec<(usize, &str)> = text.split_word_bound_indices().collect();
        if segments.is_empty() {
            return (para.range.start, para.range.start);
        }
        let k = segments
            .iter()
            .position(|(s, _)| *s == local)
            .or_else(|| segments.iter().position(|(s, w)| *s < local && local < s + w.len()))
            .unwrap_or(segments.len() - 1);
        let is_space = |w: &str| w.chars().all(char::is_whitespace);
        let k = if is_space(segments[k].1) && k > 0 && !is_space(segments[k - 1].1) { k - 1 } else { k };
        let (s, w) = segments[k];
        (para.range.start + s, para.range.start + s + w.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::ParagraphStyle;
    use crate::text::{TextEnv, layout_for_test};
    use pebbles_foundation::TextAlign;
    use std::rc::Rc;

    fn lay(text: &str, family: &str, size: f32, width: Option<f32>) -> Rc<TextLayout> {
        let style =
            ParagraphStyle { font_family: Some(family.into()), font_size: size, ..ParagraphStyle::default() };
        layout_for_test(&mut TextEnv::new(), text, &style, &[], width)
    }

    fn mono(text: &str) -> Rc<TextLayout> {
        lay(text, "JetBrains Mono", 20.0, None)
    }

    #[test]
    fn snap_inside_multibyte() {
        assert_eq!(snap("aé", 2), 1);
        assert_eq!(snap("aé", 99), 3);
    }

    #[test]
    fn para_of_boundaries() {
        let l = mono("ab\r\ncd");
        assert_eq!(l.para_of(0), (0, 0));
        assert_eq!(l.para_of(2), (0, 2));
        assert_eq!(l.para_of(3), (0, 2), "inside the CRLF ending");
        assert_eq!(l.para_of(4), (1, 0));
        assert_eq!(l.para_of(6), (1, 2));
    }

    #[test]
    fn line_for_wrap_boundary_downstream_is_next_line() {
        let l = lay("aaaa bbbb cccc", "JetBrains Mono", 20.0, Some(100.0));
        assert_eq!(l.line_for(5, Affinity::Downstream), 1);
        assert_eq!(l.line_for(5, Affinity::Upstream), 0);
    }

    #[test]
    fn line_for_end_of_para_is_last_line() {
        let l = lay("aaaa bbbb cccc", "JetBrains Mono", 20.0, Some(100.0));
        assert_eq!(l.line_for(14, Affinity::Downstream), 2);
    }

    #[test]
    fn empty_paragraph_has_one_line() {
        let l = mono("ab\n\ncd");
        assert_eq!(l.line_for(3, Affinity::Downstream), 1);
        assert_eq!(l.lines.iter().filter(|line| line.para == 1).count(), 1);
    }

    #[test]
    fn caret_x_is_advance() {
        assert!((mono("abcdefghij").caret_rect(3, Affinity::Downstream, 1.5).x0 - 36.0).abs() <= 0.01);
    }

    #[test]
    fn caret_wrap_boundary_affinity() {
        let l = lay("aaaa bbbb cccc", "JetBrains Mono", 20.0, Some(100.0));
        assert!(l.caret_rect(5, Affinity::Downstream, 1.0).y0 > l.caret_rect(5, Affinity::Upstream, 1.0).y0);
    }

    #[test]
    fn caret_empty_centered() {
        let style = ParagraphStyle { align: TextAlign::Center, ..ParagraphStyle::default() };
        let l = layout_for_test(&mut TextEnv::new(), "", &style, &[], Some(100.0));
        assert!((l.caret_rect(0, Affinity::Downstream, 1.0).x0 - 50.0).abs() <= 0.01);
    }

    #[test]
    fn selection_mixed_bidi_splits() {
        let l = lay("abc \u{5d0}\u{5d1}\u{5d2} def", "Inter", 16.0, None);
        let mut rects = Vec::new();
        l.selection_rects(2, 6, &mut rects);
        assert!(rects.len() >= 2, "{rects:?}");
        assert!(rects.iter().all(|r| r.y0 == rects[0].y0));
    }

    #[test]
    fn selection_empty_paragraph_stub() {
        let l = mono("ab\n\ncd");
        let mut rects = Vec::new();
        l.selection_rects(0, 6, &mut rects);
        let middle = l.lines[1].top;
        assert!(
            rects.iter().any(|r| r.y0 == f64::from(middle) && (r.width() - 6.0).abs() < 1e-9),
            "{rects:?}"
        );
    }

    #[test]
    fn selection_wrapped_three_lines() {
        let l = lay("aaaa bbbb cccc", "JetBrains Mono", 20.0, Some(100.0));
        let mut rects = Vec::new();
        l.selection_rects(1, 13, &mut rects);
        assert!(rects.iter().all(|r| r.width() >= 0.0));
        let mut ys: Vec<i64> = rects.iter().map(|r| r.y0 as i64).collect();
        ys.dedup();
        assert_eq!(ys.len(), 3, "{rects:?}");
    }

    #[test]
    fn hit_halves() {
        let l = mono("abcdefghij");
        assert_eq!(l.hit(37.0, 10.0), (3, Affinity::Downstream));
        assert_eq!(l.hit(43.0, 10.0), (4, Affinity::Upstream));
    }

    #[test]
    fn hit_outside() {
        let l = mono("abcdefghij");
        assert_eq!(l.hit(-100.0, -100.0), (0, Affinity::Downstream));
        assert_eq!(l.hit(1e6, 1e6), (10, Affinity::Upstream));
    }

    #[test]
    fn hit_emoji_cluster_right_half() {
        let text = "a\u{1F44D}\u{1F3FD}b";
        let l = lay(text, "Inter", 16.0, None);
        let a = l.caret_rect(1, Affinity::Downstream, 0.0).x0;
        let b = l.caret_rect(text.len() - 1, Affinity::Upstream, 0.0).x0;
        let (byte, _) = l.hit((a + (b - a) * 0.8) as f32, 8.0);
        assert_eq!(byte, text.len() - 1, "past the whole cluster");
    }

    #[test]
    fn word_at_hello_world() {
        let l = lay("hello world", "JetBrains Mono", 20.0, None);
        // Mono 12 px advances: space spans 60..72, 'w' 72..84.
        assert_eq!(l.word_at(78.0, 10.0), (6, 11));
        assert_eq!(l.word_at(63.0, 10.0), (0, 5));
        assert_eq!(l.word_at(69.0, 10.0), (6, 11));
    }
}
