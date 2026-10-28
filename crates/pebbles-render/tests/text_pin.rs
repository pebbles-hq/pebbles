//! Text-metrics pin (03 §3.7.2): the pre-swap build's paragraph metrics, recorded once (T01)
//! into a frozen oracle fixture that the cosmic-text build reproduces up to the documented
//! quirks (03 §3.13). Bundled fonts only, so the comparison is identical on every OS.

use pebbles_foundation::{Size, TextAlign};
use pebbles_render::constraints::BoxConstraints;
use pebbles_render::objects::{ParagraphStyle, RenderBaseline, RenderParagraph, TextSpanStyle};
use pebbles_render::text::TextEnv;
use pebbles_render::text_edit;
use pebbles_render::tree::RenderTree;

/// One recorded case.
#[derive(Debug)]
pub struct PinRecord {
    name: &'static str,
    width: f64,
    height: f64,
    baseline: f64,
    line_tops: &'static [f64],
    line_starts: &'static [usize],
}

include!("fixtures/text_pin_oracle.rs");

struct PinCase {
    name: &'static str,
    text: &'static str,
    family: &'static str,
    size: f32,
    line_height: f32,
    weight: f32,
    italic: bool,
    letter_spacing: f32,
    align: TextAlign,
    soft_wrap: bool,
    width: Option<f64>,
    max_lines: Option<u32>,
    ellipsis: bool,
    spans: fn() -> Vec<TextSpanStyle>,
}

const INTER: &str = "Inter";
const MONO: &str = "JetBrains Mono";

/// Fields a known, documented divergence exempts from assertion (printed instead).
/// `"*"` matches every case.
const KNOWN_DIFFS: &[(&str /*case*/, &str /*field*/, &str /*reason*/)] = &[
    ("inter16_tab", "width", "Q-5: TAB is a space-advance stop, not a .notdef box"),
    ("lora16_crlf", "height", "Q-8: CRLF is one paragraph break, not two"),
    ("lora16_crlf", "line_tops", "Q-8: CRLF is one paragraph break, not two"),
    (
        "mono20_maxlines_noell",
        "line_tops",
        "Q-2: the oracle keeps max_lines-dropped lines in its layout (hidden carets); TextLayout drops them",
    ),
    (
        "mono20_maxlines_noell",
        "line_starts",
        "Q-2: the oracle keeps max_lines-dropped lines in its layout (hidden carets); TextLayout drops them",
    ),
    ("lora16_crlf", "line_starts", "Q-8: CRLF is one paragraph break, not two"),
    (
        "inter12_separators",
        "width",
        "Q-13: U+0085 (NEL) is a paragraph break (the oracle draws it as a glyph)",
    ),
    ("inter12_separators", "height", "Q-13: U+0085 (NEL) is a paragraph break"),
    ("inter12_separators", "line_tops", "Q-13: U+0085 (NEL) is a paragraph break"),
    ("inter12_separators", "line_starts", "Q-13: U+0085 (NEL) is a paragraph break"),
    (
        "inter16_spans",
        "baseline",
        "Q-14: a mixed-size line places its baseline by the CSS/Flutter half-leading model",
    ),
    // Q-15: a paragraph's trailing whitespace is real advance (Flutter's maxIntrinsicWidth);
    // the pre-swap oracle dropped it, so "Press " ran into the next widget.
    ("inter16_trailing_ws", "width", "Q-15: trailing whitespace is kept in the width (Flutter)"),
    ("mono20_trailing_ws", "width", "Q-15: trailing whitespace is kept in the width (Flutter)"),
];

/// Q-9: the oracle pixel-snaps line tops and baselines and starts the first line box at y = -1;
/// TextLayout keeps them fractional (Flutter/Skia). The pinned value is exactly
/// `round(ours) - 1`, which `text_pin_layout_matches_oracle` asserts instead of skipping.
fn oracle_snap(v: f64) -> f64 {
    v.round() - 1.0
}

/// The Q-9 relation with the pin's ±0.25 baseline tolerance: a fractional value near .5 may
/// round either way.
fn snaps_to(v: f64, pinned: f64) -> bool {
    oracle_snap(v - 0.25) == pinned || oracle_snap(v + 0.25) == pinned
}

fn known_diff(case: &str, field: &str) -> Option<&'static str> {
    KNOWN_DIFFS.iter().find(|(c, f, _)| (*c == case || *c == "*") && *f == field).map(|(_, _, why)| *why)
}

const fn case(name: &'static str, text: &'static str, family: &'static str, size: f32) -> PinCase {
    PinCase {
        name,
        text,
        family,
        size,
        line_height: 1.2,
        weight: 400.0,
        italic: false,
        letter_spacing: 0.0,
        align: TextAlign::Start,
        soft_wrap: true,
        width: None,
        max_lines: None,
        ellipsis: false,
        spans: Vec::new,
    }
}

fn spans_bold_and_big() -> Vec<TextSpanStyle> {
    let mut bold = TextSpanStyle::new(6..10);
    bold.weight = Some(700.0);
    let mut big = TextSpanStyle::new(11..16);
    big.size = Some(24.0);
    vec![bold, big]
}

fn corpus() -> Vec<PinCase> {
    vec![
        case("inter16_hello", "Hello", INTER, 16.0),
        PinCase {
            width: Some(120.0),
            ..case("inter14_pangram_w120", "The quick brown fox jumps over the lazy dog", INTER, 14.0)
        },
        case("mono20_ascii", "abcdefghij", MONO, 20.0),
        case("inter16_trailing_ws", "abc   ", INTER, 16.0),
        case("mono20_trailing_ws", "abc   ", MONO, 20.0),
        case("mono20_tab", "a\tb", MONO, 20.0),
        case("inter16_tab", "a\tb", INTER, 16.0),
        case("inter16_trailing_nl", "line1\nline2\n", INTER, 16.0),
        PinCase { weight: 700.0, ..case("inter16_bold", "Bold", INTER, 16.0) },
        PinCase { italic: true, ..case("grotesk16_italic", "Italic", "Space Grotesk", 16.0) },
        case("inter16_bidi", "abc אבג def", INTER, 16.0),
        PinCase { letter_spacing: 2.0, ..case("mono20_ls2", "abcdef", MONO, 20.0) },
        PinCase {
            width: Some(100.0),
            max_lines: Some(2),
            ellipsis: true,
            ..case("mono20_ellipsis_wrap", "aaaa bbbb cccc", MONO, 20.0)
        },
        PinCase {
            soft_wrap: false,
            max_lines: Some(2),
            ellipsis: true,
            ..case("mono20_ellipsis_paras", "one\ntwo\nthree", MONO, 20.0)
        },
        PinCase {
            soft_wrap: false,
            max_lines: Some(1),
            ..case("mono20_maxlines_noell", "a\nbbbbbbbb\nc", MONO, 20.0)
        },
        PinCase {
            soft_wrap: false,
            align: TextAlign::Center,
            ..case("mono20_center_paras", "ab\nabcd", MONO, 20.0)
        },
        PinCase {
            width: Some(84.0),
            align: TextAlign::Justify,
            ..case("mono20_justify", "aa bb cc", MONO, 20.0)
        },
        case("lora16_crlf", "a\r\nb", "Lora", 16.0),
        case("inter16_empty", "", INTER, 16.0),
        case("mono20_override", "ab\u{202E}cd\u{202C}ef", MONO, 20.0),
        PinCase { spans: spans_bold_and_big, ..case("inter16_spans", "Hello bold world", INTER, 16.0) },
        PinCase { width: Some(100.0), ..case("mono20_long_word", "aaaaaaaaaaaaaaaa bb", MONO, 20.0) },
        case("inter12_separators", "a\u{2029}b\u{85}c", INTER, 12.0),
    ]
}

struct Measured {
    width: f64,
    height: f64,
    baseline: f64,
    line_tops: Vec<f64>,
    line_starts: Vec<usize>,
}

fn measure(env: &mut TextEnv, index: usize, c: &PinCase) -> Measured {
    let style = ParagraphStyle {
        font_size: c.size,
        line_height: c.line_height,
        weight: c.weight,
        italic: c.italic,
        letter_spacing: c.letter_spacing,
        align: c.align,
        soft_wrap: c.soft_wrap,
        max_lines: c.max_lines,
        ellipsis: c.ellipsis,
        font_family: Some(c.family.into()),
        ..ParagraphStyle::default()
    };
    let id = 9000 + index as u64;
    let mut para = RenderParagraph::with_spans(c.text, style, (c.spans)());
    para.select_id = Some(id);
    let mut tree = RenderTree::new();
    let root = tree.insert(Box::new(RenderBaseline::new(100.0)));
    let p = tree.insert(Box::new(para));
    tree.insert_child(root, p, 0);
    tree.root = Some(root);
    let constraints = match c.width {
        Some(w) => BoxConstraints::loose(Size::new(w, 10_000.0)),
        None => BoxConstraints::UNBOUNDED,
    };
    tree.layout(env, constraints);
    let size = tree.size_of(p);
    let boundaries = c.text.char_indices().map(|(b, _)| b).chain(std::iter::once(c.text.len()));
    let mut line_tops: Vec<f64> = Vec::new();
    let mut line_starts: Vec<usize> = Vec::new();
    let mut last_y0 = f64::NEG_INFINITY;
    for b in boundaries {
        let r = text_edit::caret_rect(id, b).unwrap_or_else(|| panic!("{}: no caret at {b}", c.name));
        if r.y0 > last_y0 {
            if last_y0 != f64::NEG_INFINITY {
                line_starts.push(b);
            }
            last_y0 = r.y0;
        }
        if !line_tops.contains(&r.y0) {
            line_tops.push(r.y0);
        }
    }
    line_tops.sort_by(f64::total_cmp);
    text_edit::clear(id);
    Measured {
        width: size.width,
        height: size.height,
        baseline: 100.0 - tree.offset_of(p).to_point().y,
        line_tops,
        line_starts,
    }
}

#[test]
fn text_pin_matches_oracle() {
    let mut env = TextEnv::new();
    let cases = corpus();
    let rows: Vec<(&str, Measured)> =
        cases.iter().enumerate().map(|(i, c)| (c.name, measure(&mut env, i, c))).collect();
    assert!(
        std::env::var("PEBBLES_UPDATE_TEXT_PIN").is_err(),
        "the oracle is frozen (T40): never regenerate it"
    );
    assert!(!ORACLE.is_empty(), "fixture empty: the frozen oracle is missing");
    assert_eq!(ORACLE.len(), rows.len(), "fixture and corpus disagree on the case count");
    let mut failures = Vec::new();
    for (pin, (name, m)) in ORACLE.iter().zip(&rows) {
        assert_eq!(pin.name, *name, "fixture order");
        let mut check = |field: &str, ok: bool, detail: String| {
            if ok {
                return;
            }
            if let Some(why) = known_diff(name, field) {
                println!("KNOWN_DIFF {name}.{field}: {detail} ({why})");
            } else {
                failures.push(format!("{name}.{field}: {detail}"));
            }
        };
        check("width", (m.width - pin.width).abs() <= 0.75, format!("{} vs pinned {}", m.width, pin.width));
        check(
            "height",
            (m.height - pin.height).abs() <= 0.01,
            format!("{} vs pinned {}", m.height, pin.height),
        );
        check(
            "baseline",
            snaps_to(m.baseline, pin.baseline),
            format!("{} vs pinned {} (Q-9 snap)", m.baseline, pin.baseline),
        );
        check(
            "line_tops",
            m.line_tops.len() == pin.line_tops.len()
                && m.line_tops.iter().zip(pin.line_tops).all(|(a, b)| oracle_snap(*a) == *b),
            format!("{:?} vs pinned {:?} (Q-9 snap)", m.line_tops, pin.line_tops),
        );
        check(
            "line_starts",
            m.line_starts == pin.line_starts,
            format!("{:?} vs pinned {:?}", m.line_starts, pin.line_starts),
        );
    }
    assert!(failures.is_empty(), "text metrics drifted from the pin:\n{}", failures.join("\n"));
}

/// Pinned cases the direct `TextLayout` path covers before the RenderParagraph swap (T22).
const LAYOUT_CASES: &[&str] = &[
    "inter16_hello",
    "mono20_ascii",
    "mono20_trailing_ws",
    "mono20_tab",
    "mono20_ls2",
    "mono20_override",
    "mono20_long_word",
    "inter16_trailing_nl",
    "lora16_crlf",
    "inter16_empty",
    "mono20_center_paras",
    "mono20_justify",
    "mono20_ellipsis_wrap",
    "mono20_ellipsis_paras",
    "mono20_maxlines_noell",
];

#[test]
fn text_pin_layout_matches_oracle() {
    assert!(!ORACLE.is_empty(), "fixture empty: the frozen oracle is missing");
    let mut env = TextEnv::new();
    let cases = corpus();
    let mut failures = Vec::new();
    for name in LAYOUT_CASES {
        let c = cases.iter().find(|c| c.name == *name).expect("case in corpus");
        let pin = ORACLE.iter().find(|p| p.name == *name).expect("case in fixture");
        let style = ParagraphStyle {
            font_size: c.size,
            line_height: c.line_height,
            weight: c.weight,
            italic: c.italic,
            letter_spacing: c.letter_spacing,
            align: c.align,
            soft_wrap: c.soft_wrap,
            max_lines: c.max_lines,
            ellipsis: c.ellipsis,
            font_family: Some(c.family.into()),
            ..ParagraphStyle::default()
        };
        let l = pebbles_render::text::layout_for_test(
            &mut env,
            c.text,
            &style,
            &(c.spans)(),
            c.width.map(|w| w as f32),
        );
        // The pin measured RenderParagraph, which constrains the layout to its box.
        let width = c.width.map_or(f64::from(l.width()), |w| f64::from(l.width()).min(w));
        let tops: Vec<f64> = l.line_tops().into_iter().map(f64::from).collect();
        let checks = [
            ("width", (width - pin.width).abs() <= 0.75, format!("{width} vs pinned {}", pin.width)),
            (
                "height",
                (f64::from(l.height()) - pin.height).abs() <= 0.01,
                format!("{} vs pinned {}", l.height(), pin.height),
            ),
            (
                "baseline",
                l.first_baseline().is_some_and(|b| snaps_to(f64::from(b), pin.baseline)),
                format!("{:?} vs pinned {} (Q-9 snap)", l.first_baseline(), pin.baseline),
            ),
            (
                "line_tops",
                tops.len() == pin.line_tops.len()
                    && tops.iter().zip(pin.line_tops).all(|(a, b)| oracle_snap(*a) == *b),
                format!("{tops:?} vs pinned {:?} (Q-9 snap)", pin.line_tops),
            ),
        ];
        for (field, ok, detail) in checks {
            match (ok, known_diff(name, field)) {
                (true, _) => {}
                (false, Some(why)) => println!("KNOWN_DIFF {name}.{field}: {detail} ({why})"),
                (false, None) => failures.push(format!("{name}.{field}: {detail}")),
            }
        }
    }
    assert!(failures.is_empty(), "TextLayout drifted from the pin:\n{}", failures.join("\n"));
}
