//! Editing pin (03b E01): the pre-swap build's caret rects, keyboard motions, hit-tests and
//! word selection, recorded once into a frozen oracle fixture that the TextLayout editing
//! geometry reproduces up to the documented quirks (E03–E10). Bundled fonts only.

use pebbles_foundation::Size;
use pebbles_render::constraints::BoxConstraints;
use pebbles_render::objects::{ParagraphStyle, RenderParagraph};
use pebbles_render::text::TextEnv;
use pebbles_render::text_edit;
use pebbles_render::tree::RenderTree;

/// One recorded case.
pub struct EditRecord {
    name: &'static str,
    /// (byte, x0, y0) of `caret_rect` at every char boundary.
    carets: &'static [(usize, f64, f64)],
    /// (motion, from byte, resulting focus) for every char boundary, extend = false.
    motions: &'static [(&'static str, usize, usize)],
    /// (x, y, byte) on a 4-px grid over the layout box.
    hits: &'static [(f64, f64, usize)],
    /// (x, y, start, end) of `word_at` at every glyph center.
    words: &'static [(f64, f64, usize, usize)],
}

include!("fixtures/edit_pin_parley.rs");

struct EditCase {
    name: &'static str,
    text: &'static str,
    family: &'static str,
    size: f32,
    width: Option<f64>,
}

fn corpus() -> Vec<EditCase> {
    vec![
        EditCase {
            name: "mono20_abc",
            text: "abcdefghij",
            family: "JetBrains Mono",
            size: 20.0,
            width: None,
        },
        EditCase {
            name: "inter16_wrap",
            text: "The quick brown fox jumps over the dog",
            family: "Inter",
            size: 16.0,
            width: Some(120.0),
        },
        EditCase { name: "inter16_two_paras", text: "ab\n\ncd", family: "Inter", size: 16.0, width: None },
        EditCase { name: "lora16_crlf", text: "ab\r\ncd", family: "Lora", size: 16.0, width: None },
        EditCase { name: "mixed_bidi", text: "abc אבג def", family: "Inter", size: 16.0, width: None },
        // cjk_line waits for V22's bundled CJK test font.
        EditCase { name: "emoji_cluster", text: "a👍🏽b", family: "Inter", size: 16.0, width: None },
    ]
}

type Motion = fn(u64, usize, usize, bool) -> Option<(usize, usize)>;

const MOTIONS: &[(&str, Motion)] = &[
    ("left", text_edit::left),
    ("right", text_edit::right),
    ("word_left", text_edit::word_left),
    ("word_right", text_edit::word_right),
    ("line_start", text_edit::line_start),
    ("line_end", text_edit::line_end),
    ("line_up", text_edit::line_up),
    ("line_down", text_edit::line_down),
];

struct Measured {
    carets: Vec<(usize, f64, f64)>,
    motions: Vec<(&'static str, usize, usize)>,
}

/// Which build path publishes the case for `text_edit`.
#[derive(Clone, Copy, PartialEq)]
enum Path {
    /// RenderParagraph.
    Paragraph,
    /// A cosmic-text TextLayout published with `text_edit::store_text` (E11).
    TextLayout,
}

fn publish(env: &mut TextEnv, id: u64, c: &EditCase, path: Path) -> Size {
    let style =
        ParagraphStyle { font_size: c.size, font_family: Some(c.family.into()), ..ParagraphStyle::default() };
    if path == Path::TextLayout {
        let layout =
            pebbles_render::text::layout_for_test(env, c.text, &style, &[], c.width.map(|w| w as f32));
        let size = Size::new(f64::from(layout.width()), f64::from(layout.height()));
        text_edit::store(id, layout);
        return size;
    }
    let mut para = RenderParagraph::new(c.text, style);
    para.select_id = Some(id);
    let mut tree = RenderTree::new();
    let p = tree.insert(Box::new(para));
    tree.root = Some(p);
    let constraints = match c.width {
        Some(w) => BoxConstraints::loose(Size::new(w, 10_000.0)),
        None => BoxConstraints::UNBOUNDED,
    };
    tree.layout(env, constraints);
    tree.size_of(p)
}

fn measure(env: &mut TextEnv, index: usize, c: &EditCase, path: Path) -> Measured {
    let id = 9100 + index as u64;
    publish(env, id, c, path);
    let bounds: Vec<usize> =
        c.text.char_indices().map(|(b, _)| b).chain(std::iter::once(c.text.len())).collect();
    let carets = bounds
        .iter()
        .map(|&b| {
            let r = text_edit::caret_rect(id, b).expect("caret");
            (b, r.x0, r.y0)
        })
        .collect();
    let mut motions = Vec::new();
    for (name, f) in MOTIONS {
        for &b in &bounds {
            if let Some((_, focus)) = f(id, b, b, false) {
                motions.push((*name, b, focus));
            }
        }
    }
    text_edit::clear(id);
    Measured { carets, motions }
}

fn measure_all(path: Path) -> Vec<(&'static str, Measured)> {
    let mut env = TextEnv::new();
    corpus().iter().enumerate().map(|(i, c)| (c.name, measure(&mut env, i, c, path))).collect()
}

/// Q-9 (03 §3.13): the oracle pixel-snaps line tops and starts the first line at y = -1; the
/// pinned caret y0 is exactly `round(fractional) - 1`.
fn oracle_snap(y: f64) -> f64 {
    y.round() - 1.0
}

/// Compare one build path against the pin; `snap_y` applies the Q-9 relation to caret y.
fn compare(path: Path, snap_y: bool) -> Vec<String> {
    let rows = measure_all(path);
    let mut failures = Vec::new();
    for (pin, (name, m)) in PARLEY.iter().zip(&rows) {
        assert_eq!(pin.name, *name, "fixture order");
        let close = |a: f64, b: f64| (a - b).abs() <= 0.5;
        let y = |v: f64| if snap_y { oracle_snap(v) } else { v };
        for (p, c) in pin.carets.iter().zip(&m.carets) {
            if p.0 != c.0 || !close(p.1, c.1) || !close(p.2, y(c.2)) {
                failures.push(format!(
                    "{name}.caret@{}: pinned ({}, {}) got ({}, {})",
                    p.0,
                    p.1,
                    p.2,
                    c.1,
                    y(c.2)
                ));
            }
        }
        if pin.carets.len() != m.carets.len() {
            failures.push(format!("{name}.carets: {} vs {}", pin.carets.len(), m.carets.len()));
        }
        for p in pin.motions {
            let got = m.motions.iter().find(|c| c.0 == p.0 && c.1 == p.1).map(|c| c.2);
            if got != Some(p.2) {
                failures.push(format!("{name}.{}({}): pinned {} got {got:?}", p.0, p.1, p.2));
            }
        }
        // Hits and words are sampled at pinned coordinates (shifted by the Q-9 offset).
        let id = 9200 + rows.iter().position(|r| r.0 == *name).unwrap_or(0) as u64;
        let mut env = TextEnv::new();
        let case = corpus().into_iter().find(|c| c.name == *name).expect("case");
        let size = publish(&mut env, id, &case, path);
        let dy = if snap_y { 1.0 } else { 0.0 };
        for &(x, py, b) in pin.hits {
            if py + dy >= size.height {
                continue; // the Q-9 shift moved this sample below our box
            }
            let got = text_edit::hit(id, x, py + dy);
            if got != Some(b) {
                failures.push(format!("{name}.hit({x}, {py}): pinned {b} got {got:?}"));
            }
        }
        for &(x, py, s, e) in pin.words {
            let got = text_edit::word_at(id, x, py + dy);
            if got != Some((s, e)) {
                failures.push(format!("{name}.word_at({x}, {py}): pinned {s}..{e} got {got:?}"));
            }
        }
        text_edit::clear(id);
    }
    failures
}

/// Documented, deliberate differences from the pre-swap build (03 §3.13), as
/// (case, failure-label prefix, reason). Each is printed, not asserted.
const KNOWN_DIFFS: &[(&str, &str, &str)] = &[
    ("lora16_crlf", "", "Q-8: CRLF is one paragraph break (the oracle lays out an extra line)"),
    ("emoji_cluster", "hit", "Q-10: a grapheme cluster is never split (the oracle steps inside 👍🏽)"),
    ("emoji_cluster", "left", "Q-10: a grapheme cluster is never split"),
    ("emoji_cluster", "right", "Q-10: a grapheme cluster is never split"),
    (
        "inter16_wrap",
        "left(",
        "Q-11: Left at a soft-wrap start moves on (the oracle stays: its byte API dropped affinity)",
    ),
    ("inter16_wrap", "word_left(", "Q-11: Left at a soft-wrap start moves on"),
    (
        "inter16_two_paras",
        "word_",
        "Q-12: word motion is logical UAX #29 crossing paragraphs (line-table parity)",
    ),
    ("mixed_bidi", "word_", "Q-12: word motion is logical UAX #29, not visual"),
];

/// Every difference from the pin must be a documented KNOWN_DIFF.
fn assert_matches_oracle(path: Path) {
    assert!(!PARLEY.is_empty(), "fixture empty: the frozen oracle is missing");
    let mut unexpected = Vec::new();
    for f in compare(path, true) {
        let known = KNOWN_DIFFS.iter().find(|(case, label, _)| {
            f.strip_prefix(case).and_then(|r| r.strip_prefix('.')).is_some_and(|r| r.starts_with(label))
        });
        match known {
            Some((_, _, why)) => println!("KNOWN_DIFF {f} ({why})"),
            None => unexpected.push(f),
        }
    }
    assert!(unexpected.is_empty(), "{} differences:\n{}", unexpected.len(), unexpected.join("\n"));
}

/// Through the public RenderParagraph path.
#[test]
fn edit_matches_oracle() {
    assert_matches_oracle(Path::Paragraph);
}

/// Through a TextLayout published with `store_text`.
#[test]
fn text_layout_editing_matches_oracle() {
    assert_matches_oracle(Path::TextLayout);
}
