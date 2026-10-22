//! Keyboard motion over a [`TextLayout`] (03b §3.8). Every motion returns `(anchor, focus)`;
//! `extend` keeps the anchor, otherwise both collapse onto the new focus. Every returned byte
//! is a char boundary ≤ `text.len()`.

use unicode_segmentation::UnicodeSegmentation;

use super::geometry::{Affinity, snap};
use super::layout::TextLayout;

fn apply(a: usize, f: usize, extend: bool) -> (usize, usize) {
    if extend { (a, f) } else { (f, f) }
}

/// Text byte of a line-local byte (elided lines clamp to the cut).
fn to_text(layout: &TextLayout, line: usize, local: usize) -> usize {
    let l = &layout.lines[line];
    let para = &layout.shaped.paras[l.para];
    let local = match (&layout.elided, l.from_elided) {
        (Some(e), true) => local.min(e.cut),
        _ => local,
    };
    para.range.start + local.min(para.range.len())
}

/// The visual caret stops of `line`, sorted by x: per grapheme, its leading and trailing edge.
pub(crate) fn caret_stops(layout: &TextLayout, line: usize) -> Vec<(f32, usize, Affinity)> {
    let l = &layout.lines[line];
    if !l.from_elided && layout.shaped.paras[l.para].placeholder {
        return Vec::new();
    }
    let text = layout.line_text(l);
    let mut stops = Vec::new();
    for g in &l.layout.glyphs {
        let cluster = text.get(g.start..g.end).unwrap_or("");
        let graphemes: Vec<(usize, &str)> = cluster.grapheme_indices(true).collect();
        let n = graphemes.len().max(1);
        let cw = g.w / n as f32;
        for (k, (i, egc)) in graphemes.iter().enumerate() {
            let (start, end) =
                (to_text(layout, line, g.start + i), to_text(layout, line, g.start + i + egc.len()));
            let (x0, x1) = (g.x + k as f32 * cw, g.x + (k + 1) as f32 * cw);
            if g.level.is_rtl() {
                stops.push((x0, end, Affinity::Upstream));
                stops.push((x1, start, Affinity::Downstream));
            } else {
                stops.push((x0, start, Affinity::Downstream));
                stops.push((x1, end, Affinity::Upstream));
            }
        }
    }
    stops.sort_by(|a, b| a.0.total_cmp(&b.0));
    stops
}

/// The next (forward) / previous (backward) grapheme boundary of the whole text.
fn logical_step(text: &str, f: usize, forward: bool) -> usize {
    if forward {
        text[f..].graphemes(true).next().map_or(text.len(), |g| f + g.len())
    } else {
        text[..f].grapheme_indices(true).next_back().map_or(0, |(i, _)| i)
    }
}

/// One visual step; `rightward` = the Right arrow.
fn horizontal(layout: &TextLayout, a: usize, f: usize, extend: bool, rightward: bool) -> (usize, usize) {
    let text = &layout.shaped.text;
    let (a, f) = (snap(text, a), snap(text, f));
    if !extend && a != f {
        let m = if rightward { a.max(f) } else { a.min(f) };
        return (m, m);
    }
    if layout.lines.is_empty() {
        return (a, f);
    }
    let line = layout.line_for(f, Affinity::Downstream);
    let stops = caret_stops(layout, line);
    let rtl = layout.shaped.paras[layout.lines[line].para].shape.rtl;
    let current = stops
        .iter()
        .position(|s| s.1 == f && s.2 == Affinity::Downstream)
        .or_else(|| stops.iter().position(|s| s.1 == f));
    if current.is_none() && !stops.is_empty() {
        // Not a caret stop (inside a grapheme): step to the grapheme boundary.
        return apply(a, logical_step(text, f, rightward != rtl), extend);
    }
    if let Some(cur) = current {
        // The next stop that is a different byte AND a different place on screen (bidi run
        // boundaries put two bytes at one x; stepping between them is no visible motion).
        let x = stops[cur].0;
        let moved = |s: &&(f32, usize, Affinity)| s.1 != f && (s.0 - x).abs() > 1e-3;
        let next = if rightward {
            stops[cur + 1..].iter().find(moved)
        } else {
            stops[..cur].iter().rev().find(moved)
        };
        if let Some(s) = next {
            return apply(a, s.1, extend);
        }
    }
    // The line's visual edge: logical motion in the paragraph's reading direction.
    apply(a, logical_step(text, f, rightward != rtl), extend)
}

pub(crate) fn left(layout: &TextLayout, a: usize, f: usize, extend: bool) -> (usize, usize) {
    horizontal(layout, a, f, extend, false)
}

pub(crate) fn right(layout: &TextLayout, a: usize, f: usize, extend: bool) -> (usize, usize) {
    horizontal(layout, a, f, extend, true)
}

fn is_space(w: &str) -> bool {
    w.chars().all(char::is_whitespace)
}

/// Logical word motion: skip whitespace segments, then one word segment (UAX #29).
fn word_step(text: &str, f: usize, forward: bool) -> usize {
    let segments: Vec<(usize, &str)> = text.split_word_bound_indices().collect();
    if forward {
        let mut k = segments.iter().position(|(s, w)| s + w.len() > f).unwrap_or(segments.len());
        while k < segments.len() && is_space(segments[k].1) {
            k += 1;
        }
        segments.get(k).map_or(text.len(), |(s, w)| (s + w.len()).max(f))
    } else {
        let mut k = segments.iter().rposition(|(s, _)| *s < f);
        while let Some(i) = k {
            if !is_space(segments[i].1) {
                break;
            }
            k = i.checked_sub(1);
        }
        k.map_or(0, |i| segments[i].0.min(f))
    }
}

fn word(layout: &TextLayout, a: usize, f: usize, extend: bool, rightward: bool) -> (usize, usize) {
    let text = &layout.shaped.text;
    let (a, f) = (snap(text, a), snap(text, f));
    let (p, _) = layout.para_of(f);
    let rtl = layout.shaped.paras[p].shape.rtl;
    apply(a, word_step(text, f, rightward != rtl), extend)
}

pub(crate) fn word_left(layout: &TextLayout, a: usize, f: usize, extend: bool) -> (usize, usize) {
    word(layout, a, f, extend, false)
}

pub(crate) fn word_right(layout: &TextLayout, a: usize, f: usize, extend: bool) -> (usize, usize) {
    word(layout, a, f, extend, true)
}

/// The caret's visual line's logical start (`Downstream`) or end (`Upstream`).
fn line_edge(layout: &TextLayout, a: usize, f: usize, extend: bool, end: bool) -> (usize, usize) {
    let text = &layout.shaped.text;
    let (a, f) = (snap(text, a), snap(text, f));
    if layout.lines.is_empty() {
        return (a, f);
    }
    let i = layout.line_for(f, Affinity::Downstream);
    let line = &layout.lines[i];
    let glyphs = &line.layout.glyphs;
    let para_start = layout.shaped.paras[line.para].range.start;
    let target = if layout.shaped.paras[line.para].placeholder || glyphs.is_empty() {
        para_start
    } else if end {
        to_text(layout, i, layout.line_logical_end(i))
    } else {
        to_text(layout, i, layout.line_logical_start(i))
    };
    apply(a, target, extend)
}

pub(crate) fn line_start(layout: &TextLayout, a: usize, f: usize, extend: bool) -> (usize, usize) {
    line_edge(layout, a, f, extend, false)
}

pub(crate) fn line_end(layout: &TextLayout, a: usize, f: usize, extend: bool) -> (usize, usize) {
    line_edge(layout, a, f, extend, true)
}

/// Up / down one visual line at the caret's x; from the first / last line, to the text's
/// start / end.
fn vertical(layout: &TextLayout, a: usize, f: usize, extend: bool, up: bool) -> (usize, usize) {
    let text = &layout.shaped.text;
    let (a, f) = (snap(text, a), snap(text, f));
    if layout.lines.is_empty() {
        return (a, f);
    }
    let i = layout.line_for(f, Affinity::Downstream);
    let target = if up { i.checked_sub(1) } else { Some(i + 1).filter(|&j| j < layout.lines.len()) };
    // Past the first / last line: the text's start / end (the pre-swap build, Flutter).
    let Some(t) = target else { return apply(a, if up { 0 } else { text.len() }, extend) };
    let x = layout.caret_rect(f, Affinity::Downstream, 0.0).x0 as f32;
    let line = &layout.lines[t];
    apply(a, layout.hit(x, line.top + line.height / 2.0).0, extend)
}

pub(crate) fn line_up(layout: &TextLayout, a: usize, f: usize, extend: bool) -> (usize, usize) {
    vertical(layout, a, f, extend, true)
}

pub(crate) fn line_down(layout: &TextLayout, a: usize, f: usize, extend: bool) -> (usize, usize) {
    vertical(layout, a, f, extend, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::ParagraphStyle;
    use crate::text::{TextEnv, layout_for_test};
    use std::rc::Rc;

    fn lay(text: &str, family: &str, width: Option<f32>) -> Rc<TextLayout> {
        let style =
            ParagraphStyle { font_family: Some(family.into()), font_size: 20.0, ..ParagraphStyle::default() };
        layout_for_test(&mut TextEnv::new(), text, &style, &[], width)
    }

    #[test]
    fn horizontal_ltr_and_collapse() {
        let l = lay("hello world", "JetBrains Mono", None);
        let mut f = 0;
        for _ in 0..5 {
            f = right(&l, f, f, false).1;
        }
        assert_eq!(f, 5);
        assert_eq!(left(&l, 2, 7, false), (2, 2));
        assert_eq!(right(&l, 2, 7, false), (7, 7));
    }

    #[test]
    fn horizontal_rtl_left_is_forward() {
        let l = lay("\u{5d0}\u{5d1}\u{5d2}", "Inter", None);
        assert_eq!(left(&l, 0, 0, false).1, 2);
    }

    #[test]
    fn horizontal_mixed_is_monotonic_then_crosses() {
        let text = "abc \u{5d0}\u{5d1}\u{5d2} def\nxy";
        let l = lay(text, "Inter", None);
        let mut f = 0;
        let mut last_x = f64::NEG_INFINITY;
        let first_line_end = text.find('\n').unwrap();
        let mut steps = 0;
        while l.line_for(f, Affinity::Downstream) == 0 && steps < 40 {
            let x = l.caret_rect(f, Affinity::Downstream, 0.0).x0;
            assert!(x >= last_x - 1e-3, "x went back at byte {f}: {x} < {last_x}");
            last_x = x;
            f = right(&l, f, f, false).1;
            steps += 1;
        }
        assert!(f > first_line_end, "crossed into the next paragraph (at {f})");
    }

    #[test]
    fn word_motions() {
        let l = lay("hello world", "JetBrains Mono", None);
        assert_eq!(word_right(&l, 0, 0, false).1, 5);
        assert_eq!(word_right(&l, 5, 5, false).1, 11);
        assert_eq!(word_left(&l, 11, 11, false).1, 6);
    }

    #[test]
    fn line_edges() {
        let l = lay("hello world", "JetBrains Mono", None);
        assert_eq!(line_end(&l, 3, 3, false).1, 11);
        let w = lay("aaaa bbbb cccc", "JetBrains Mono", Some(100.0));
        assert_eq!(line_end(&w, 1, 1, false).1, 5, "the first visual line's end (after its space)");
        assert_eq!(line_start(&w, 7, 7, false).1, 5);
    }

    #[test]
    fn vertical_parity() {
        let l = lay("aaaaaa\naaaa", "JetBrains Mono", None);
        // Caret at byte 4 of line 2 = text byte 11.
        assert_eq!(line_up(&l, 11, 11, false).1, 4);
        assert_eq!(line_up(&l, 2, 2, false), (0, 0));
        assert_eq!(line_down(&l, 11, 11, false), (11, 11));
        assert_eq!(line_down(&l, 8, 8, false), (11, 11));
    }
}

#[cfg(test)]
mod robustness {
    //! Seeded motion sequences (03b §3.11, E19): never panic, every focus a char boundary ≤ len.
    use super::*;
    use crate::text::TextEnv;
    use crate::text::geometry::robustness::{next, random_layout};

    const ALPHABET: &[&str] = &[
        "a",
        "Z",
        " ",
        "\u{5d0}",
        "\u{5d1}",
        "\u{627}",
        "\u{4e2d}",
        "👩\u{200d}👩\u{200d}👧",
        "👍\u{1F3FD}",
        "e\u{301}",
        "\r",
        "\n",
        "\r\n",
        "\u{2029}",
    ];

    type Motion = fn(&TextLayout, usize, usize, bool) -> (usize, usize);
    const MOTIONS: [Motion; 8] =
        [left, right, word_left, word_right, line_start, line_end, line_up, line_down];

    #[test]
    fn seeded_motion_sequences() {
        let mut env = TextEnv::new();
        for seed in 1..=2_000u64 {
            let mut state = seed.wrapping_mul(0xD1B5_4A32_D192_ED03) | 1;
            let l = random_layout(&mut env, &mut state, ALPHABET);
            let t = l.text();
            let (mut a, mut f) = ((next(&mut state) % (t.len() as u64 + 9)) as usize, 0);
            for _ in 0..50 {
                let m = MOTIONS[(next(&mut state) % 8) as usize];
                let extend = next(&mut state).is_multiple_of(2);
                (a, f) = m(&l, a, f, extend);
                for b in [a, f] {
                    assert!(b <= t.len() && t.is_char_boundary(b), "motion returned {b} in {t:?}");
                }
            }
        }
    }
}
