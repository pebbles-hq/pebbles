//! Editing pin (03b E01): the Parley build's caret rects, keyboard motions, hit-tests and
//! word selection, recorded once into an oracle fixture that the TextLayout editing geometry
//! must reproduce (E03–E10). Hermetic fonts (bundled faces only).

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
    hits: Vec<(f64, f64, usize)>,
    words: Vec<(f64, f64, usize, usize)>,
}

/// Which build path publishes the case for `text_edit`.
#[derive(Clone, Copy, PartialEq)]
enum Path {
    /// RenderParagraph (Parley until T22).
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
    let size = publish(env, id, c, path);
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
    let mut hits = Vec::new();
    let (mut y, mut x);
    y = 0.0;
    while y < size.height {
        x = 0.0;
        while x < size.width {
            if let Some(b) = text_edit::hit(id, x, y) {
                hits.push((x, y, b));
            }
            x += 4.0;
        }
        y += 4.0;
    }
    // Glyph centers: the midpoint between consecutive caret stops on the same line.
    let mut words = Vec::new();
    for w in bounds.windows(2) {
        let (a, b) = (
            text_edit::caret_rect(id, w[0]).expect("caret"),
            text_edit::caret_rect(id, w[1]).expect("caret"),
        );
        if a.y0 == b.y0 && b.x0 > a.x0 {
            let (cx, cy) = ((a.x0 + b.x0) / 2.0, (a.y0 + a.y1) / 2.0);
            if let Some((s, e)) = text_edit::word_at(id, cx, cy) {
                words.push((cx, cy, s, e));
            }
        }
    }
    text_edit::clear(id);
    Measured { carets, motions, hits, words }
}

fn write_fixture(rows: &[(&str, Measured)]) {
    let mut out = String::from(
        "// Generated by `cargo test -p pebbles-render --test edit_pin -- --ignored regenerate_edit_pin`\n\
         // on the Parley build (E01). Never regenerate after the switch (N15).\n\
         pub const PARLEY: &[EditRecord] = &[\n",
    );
    for (name, m) in rows {
        let carets: Vec<String> = m.carets.iter().map(|(b, x, y)| format!("({b}, {x:?}, {y:?})")).collect();
        let motions: Vec<String> = m.motions.iter().map(|(n, a, f)| format!("({n:?}, {a}, {f})")).collect();
        let hits: Vec<String> = m.hits.iter().map(|(x, y, b)| format!("({x:?}, {y:?}, {b})")).collect();
        let words: Vec<String> =
            m.words.iter().map(|(x, y, s, e)| format!("({x:?}, {y:?}, {s}, {e})")).collect();
        out.push_str(&format!(
            "    EditRecord {{\n        name: {name:?},\n        carets: &[{}],\n        motions: &[{}],\n        hits: &[{}],\n        words: &[{}],\n    }},\n",
            carets.join(", "),
            motions.join(", "),
            hits.join(", "),
            words.join(", ")
        ));
    }
    out.push_str("];\n");
    std::fs::write(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/edit_pin_parley.rs"), out)
        .expect("write");
}

fn measure_all(path: Path) -> Vec<(&'static str, Measured)> {
    let mut env = TextEnv::new_hermetic();
    corpus().iter().enumerate().map(|(i, c)| (c.name, measure(&mut env, i, c, path))).collect()
}

#[test]
#[ignore = "regenerates the Parley oracle; never after the switch"]
fn regenerate_edit_pin() {
    write_fixture(&measure_all(Path::Paragraph));
}

/// Q-9 (03 §3.13): Parley pixel-snaps line tops and starts the first line at y = -1; the
/// pinned caret y0 is exactly `round(fractional) - 1`.
fn parley_snap(y: f64) -> f64 {
    y.round() - 1.0
}

/// Compare one build path against the pin; `snap_y` applies the Q-9 relation to caret y.
fn compare(path: Path, snap_y: bool) -> Vec<String> {
    let rows = measure_all(path);
    let mut failures = Vec::new();
    for (pin, (name, m)) in PARLEY.iter().zip(&rows) {
        assert_eq!(pin.name, *name, "fixture order");
        let close = |a: f64, b: f64| (a - b).abs() <= 0.5;
        let y = |v: f64| if snap_y { parley_snap(v) } else { v };
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
        let mut env = TextEnv::new_hermetic();
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

/// Documented, deliberate differences from the Parley build (03 §3.13), as
/// (case, failure-label prefix, reason). Each is printed, not asserted.
const KNOWN_DIFFS: &[(&str, &str, &str)] = &[
    ("lora16_crlf", "", "Q-8: CRLF is one paragraph break (Parley lays out an extra line)"),
    ("emoji_cluster", "hit", "Q-10: a grapheme cluster is never split (Parley steps inside 👍🏽)"),
    ("emoji_cluster", "left", "Q-10: a grapheme cluster is never split"),
    ("emoji_cluster", "right", "Q-10: a grapheme cluster is never split"),
    (
        "inter16_wrap",
        "left(",
        "Q-11: Left at a soft-wrap start moves on (Parley stays: its byte API drops affinity)",
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
    assert!(!PARLEY.is_empty(), "fixture empty: run regenerate_edit_pin on the Parley build");
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

/// Through the public RenderParagraph path (cosmic-text since T22).
#[test]
fn edit_matches_parley_oracle() {
    assert_matches_oracle(Path::Paragraph);
}

/// Through a TextLayout published with `store_text`.
#[test]
fn text_layout_editing_matches_parley_oracle() {
    assert_matches_oracle(Path::TextLayout);
}
