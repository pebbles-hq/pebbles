//! Text performance probes (03 §6.4, T02/T37): hand-rolled timing, `#[ignore]`d — run in
//! `--release` with `-- --ignored --nocapture` and read the printed numbers.

use pebbles_foundation::{Size, TextAlign};
use pebbles_render::constraints::BoxConstraints;
use pebbles_render::objects::{ParagraphStyle, RenderParagraph};
use pebbles_render::text::TextEnv;
use pebbles_render::tree::RenderTree;
use web_time::Instant;

/// The T01 corpus inputs: (text, family, size, width, max_lines, ellipsis, soft_wrap, align).
type Case = (&'static str, &'static str, f32, Option<f64>, Option<u32>, bool, bool, TextAlign);

const CORPUS: &[Case] = &[
    ("Hello", "Inter", 16.0, None, None, false, true, TextAlign::Start),
    (
        "The quick brown fox jumps over the lazy dog",
        "Inter",
        14.0,
        Some(120.0),
        None,
        false,
        true,
        TextAlign::Start,
    ),
    ("abcdefghij", "JetBrains Mono", 20.0, None, None, false, true, TextAlign::Start),
    ("abc   ", "Inter", 16.0, None, None, false, true, TextAlign::Start),
    ("a\tb", "JetBrains Mono", 20.0, None, None, false, true, TextAlign::Start),
    ("line1\nline2\n", "Inter", 16.0, None, None, false, true, TextAlign::Start),
    ("Italic", "Space Grotesk", 16.0, None, None, false, true, TextAlign::Start),
    ("abc אבג def", "Inter", 16.0, None, None, false, true, TextAlign::Start),
    ("aaaa bbbb cccc", "JetBrains Mono", 20.0, Some(100.0), Some(2), true, true, TextAlign::Start),
    ("one\ntwo\nthree", "JetBrains Mono", 20.0, None, Some(2), true, false, TextAlign::Start),
    ("ab\nabcd", "JetBrains Mono", 20.0, None, None, false, false, TextAlign::Center),
    ("aa bb cc", "JetBrains Mono", 20.0, Some(84.0), None, false, true, TextAlign::Justify),
    ("a\r\nb", "Lora", 16.0, None, None, false, true, TextAlign::Start),
    ("aaaaaaaaaaaaaaaa bb", "JetBrains Mono", 20.0, Some(100.0), None, false, true, TextAlign::Start),
];

fn lay(env: &mut TextEnv, text: String, c: &Case, width: Option<f64>) {
    let style = ParagraphStyle {
        font_family: Some(c.1.into()),
        font_size: c.2,
        max_lines: c.4,
        ellipsis: c.5,
        soft_wrap: c.6,
        align: c.7,
        ..ParagraphStyle::default()
    };
    let mut tree = RenderTree::new();
    let p = tree.insert(Box::new(RenderParagraph::new(text, style)));
    tree.root = Some(p);
    let constraints = match width {
        Some(w) => BoxConstraints::loose(Size::new(w, 1e9)),
        None => BoxConstraints::UNBOUNDED,
    };
    tree.layout(env, constraints);
}

#[allow(deprecated)] // the pre-swap build named the bundled-only env `new_hermetic`
fn env() -> TextEnv {
    TextEnv::new_hermetic()
}

#[test]
#[ignore = "timing probe; run in --release"]
fn text_perf_corpus() {
    let mut env = env();
    let t = Instant::now();
    for i in 0..1_000 {
        for c in CORPUS {
            lay(&mut env, format!("{} #{i}", c.0), c, c.3);
        }
        env.finish_frame();
    }
    println!("corpus_ms={}", t.elapsed().as_secs_f64() * 1e3);
}

#[test]
#[ignore = "timing probe; run in --release"]
fn text_perf_cold_document() {
    let mut doc = String::new();
    let sentence = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. ";
    while doc.len() < 160 * 1024 {
        doc.push_str(sentence);
        if doc.len() % 400 < sentence.len() {
            doc.push('\n');
        }
    }
    let case: Case = ("", "Inter", 16.0, None, None, false, true, TextAlign::Start);
    let mut env = env();
    let style_tree = |env: &mut TextEnv, w: f64| {
        let style = ParagraphStyle {
            font_family: Some(case.1.into()),
            font_size: case.2,
            ..ParagraphStyle::default()
        };
        let mut tree = RenderTree::new();
        let p = tree.insert(Box::new(RenderParagraph::new(doc.clone(), style)));
        tree.root = Some(p);
        let t = Instant::now();
        tree.layout(env, BoxConstraints::loose(Size::new(w, 1e9)));
        t.elapsed().as_secs_f64() * 1e3
    };
    println!("cold_doc_ms={}", style_tree(&mut env, 800.0));
    println!("rewrap_ms={}", style_tree(&mut env, 600.0));
}
