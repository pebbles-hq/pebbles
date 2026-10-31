//! text/ Latin — Slice P2 (glyph rendering), §7.2, V21. Bundled families only (Inter,
//! JetBrains Mono, Space Grotesk, Lora); judged against the golden model's glyph outlines.

use pebbles_core::IntoWidget as _;
use pebbles_foundation::{Color, EdgeInsets};
use pebbles_render::paint::{Painter, kurbo, peniko};
use pebbles_testing::Harness;
use pebbles_widgets::{column, container, span, text, text_field, text_rich, transform};

use kurbo::Affine;

use crate::case::{Case, Category::Text, Slice::P2, Slice::TrackT, harness, ops, with_scale_variants};

const W: u32 = 480;
const H: u32 = 320;
const PANGRAM: &str = "The quick brown fox jumps over the lazy dog 0123456789";
const LOREM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor \
    incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation \
    ullamco laboris nisi ut aliquip ex ea commodo consequat. ";

pub fn cases() -> Vec<Case> {
    let mut v = with_scale_variants(harness("text/latin_sizes", Text, P2, W, H, latin_sizes));
    v.extend([
        harness("text/large_96", Text, P2, 640, 160, large_96),
        harness("text/weights", Text, P2, W, H, weights),
        harness("text/italic_synth", Text, P2, W, H, italic_synth),
        harness("text/mono_tabs", Text, P2, W, H, mono_tabs),
        harness("text/serif", Text, P2, W, H, serif),
        harness("text/colored_runs", Text, P2, W, H, colored_runs),
        harness("text/decorations", Text, P2, W, H, decorations),
        harness("text/ellipsis_maxlines", Text, P2, W, H, ellipsis_maxlines),
        harness("text/partially_offscreen_top", Text, P2, W, H, partially_offscreen_top),
    ]);
    v.extend(with_scale_variants(harness("text/subpixel_positions", Text, P2, W, H, subpixel_positions)));
    v.extend([
        harness("text/tiny_6px", Text, P2, W, H, tiny_6px),
        harness("text/dense_paragraph", Text, P2, W, H, dense_paragraph),
        // Gravel "blurry vs Zed": 13 px UI + 13/14 px mono, light-on-dark and dark-on-light at 1×.
        harness("text/sharpness_13px", Text, P2, W, H, sharpness_13px),
        // The same text with subpixel (LCD) antialiasing (dark + light blocks); probes only.
        harness("text/lcd_13px", Text, P2, W, H, sharpness_13px)
            .subpixel_text()
            .probe("all_opaque", crate::probe::all_opaque)
            .probe("lcd_fringes_neutral", crate::probe::lcd_fringes_neutral),
        harness("text/rotated_30", Text, P2, W, H, rotated_30),
        harness("text/scaled_transform_3x", Text, P2, W, H, scaled_transform_3x),
        harness("text/selection_caret", Text, P2, W, H, selection_caret),
        ops("text/glyph_run_synthetic", Text, P2, W, H, glyph_run_synthetic),
        // V19: three fonts drawing the SAME glyph ids at the same size and pen bins in one scene,
        // so a glyph cache that ignores the font bytes (the C09 defect) draws the wrong shapes.
        ops("text/two_fonts_same_glyph_ids", Text, P2, W, H, two_fonts_same_glyph_ids),
        // R42 text shadow: the run's union blurred once (V53 case).
        ops("text/shadow_blur", Text, P2, 320, 96, shadow_blur),
        // COLR (R29): the committed colr-test.ttf, judged by the golden COLR evaluator (V51).
        ops("text/colr_v0_layers", Text, P2, 160, 160, colr_v0_layers),
        ops("text/colr_v0_rotated", Text, P2, 160, 160, colr_v0_rotated),
        ops("text/colr_foreground", Text, P2, 160, 160, colr_foreground),
        // COLRv1 gradients render through the gradient pipeline (L35).
        ops("text/colr_v1_gradient", Text, P2, 160, 160, colr_v1_gradient),
        // COLRv1 composite modes render through the blend layers of L21–L24.
        ops("text/colr_v1_composite", Text, P2, 160, 160, colr_v1_composite),
    ]);
    v
}

/// text/ international — Slice TrackT (V22): the committed Noto test fonts (`fonts/`).
pub fn international() -> Vec<Case> {
    crate::register_test_fonts();
    vec![
        harness("text/rtl_arabic", Text, TrackT, W, H, rtl_arabic),
        harness("text/rtl_hebrew", Text, TrackT, W, H, rtl_hebrew),
        harness("text/bidi_mixed", Text, TrackT, W, H, bidi_mixed),
        harness("text/cjk", Text, TrackT, W, H, cjk),
        harness("text/emoji_color", Text, TrackT, W, H, emoji_color)
            .probe("emoji_has_color", crate::probe::emoji_has_color),
        harness("text/mixed_scripts_fallback", Text, TrackT, W, H, mixed_scripts_fallback),
        harness("text/decorations_all_styles", Text, TrackT, W, H, decorations_all_styles),
        harness("text/shadows_and_background", Text, TrackT, W, H, shadows_and_background),
    ]
}

/// T43: every decoration style, plus overline, a decoration color and a thickness multiplier.
fn decorations_all_styles(h: &mut Harness) {
    use pebbles_render::TextDecorationStyle as S;
    let row = |label: &'static str, style: S| {
        text_rich(vec![span(label).underline().decoration_style(style)]).size(20.0).into_widget()
    };
    page(
        h,
        column(vec![
            row("Solid underline", S::Solid),
            row("Double underline", S::Double),
            row("Dotted underline", S::Dotted),
            row("Dashed underline", S::Dashed),
            row("Wavy underline", S::Wavy),
            text_rich(vec![
                span("Overline ").overline(),
                span("red, thick ")
                    .underline()
                    .decoration_color(Color::from_rgba8(220, 38, 38, 255))
                    .decoration_thickness(3.0),
                span("struck").strikethrough().decoration_style(S::Wavy),
            ])
            .size(20.0)
            .into_widget(),
        ])
        .spacing(10.0),
    );
}

/// T44 / T45: blurred and sharp glyph shadows, and a span background over its line box.
fn shadows_and_background(h: &mut Harness) {
    use pebbles_foundation::Offset;
    use pebbles_render::TextShadow;
    let soft =
        TextShadow { offset: Offset::new(2.0, 3.0), blur_sigma: 3.0, color: Color::from_rgba8(0, 0, 0, 140) };
    let hard = TextShadow {
        offset: Offset::new(2.0, 2.0),
        blur_sigma: 0.0,
        color: Color::from_rgba8(37, 99, 235, 255),
    };
    page(
        h,
        column(vec![
            text_rich(vec![span("Soft shadow").shadow(soft)]).size(36.0).into_widget(),
            text_rich(vec![span("Hard shadow").shadow(hard)]).size(36.0).into_widget(),
            text_rich(vec![
                span("plain "),
                span("highlighted").background(Color::from_rgba8(253, 224, 71, 255)),
                span(" text"),
            ])
            .size(24.0)
            .into_widget(),
        ])
        .spacing(12.0),
    );
}

fn rtl_arabic(h: &mut Harness) {
    page(h, text("مرحبا بالعالم").font_family("Noto Sans Arabic").size(28.0));
}

fn rtl_hebrew(h: &mut Harness) {
    page(h, text("שלום עולם").font_family("Noto Sans Hebrew").size(28.0));
}

fn bidi_mixed(h: &mut Harness) {
    // Inter has no Hebrew: the run falls back to the registered Noto Sans Hebrew.
    page(h, text("abc אבג 123 def").size(28.0));
}

fn cjk(h: &mut Harness) {
    page(h, text("你好，世界。渲染测试").font_family("Noto Sans CJK SC").size(28.0));
}

fn emoji_color(h: &mut Harness) {
    page(h, text("😀 👍 🚀 🎉").font_family("Noto Color Emoji").size(40.0));
}

fn mixed_scripts_fallback(h: &mut Harness) {
    page(h, text("Hello مرحبا 你好").size(28.0));
}

fn page(h: &mut Harness, child: impl pebbles_core::IntoWidget) {
    h.mount_widget(container().padding(EdgeInsets::all(16.0)).child(child));
}

fn latin_sizes(h: &mut Harness) {
    let rows: Vec<_> =
        [10.0, 12.0, 14.0, 16.0, 24.0].iter().map(|s| text(PANGRAM).size(*s).into_widget()).collect();
    page(h, column(rows).spacing(8.0));
}

fn large_96(h: &mut Harness) {
    page(h, text("Hamburgefonstiv").size(96.0));
}

fn weights(h: &mut Harness) {
    let rows: Vec<_> = (1..=9)
        .map(|k| text(format!("Weight {} — Hamburgefonstiv", k * 100)).weight(k as f32 * 100.0).into_widget())
        .collect();
    page(h, column(rows).spacing(4.0));
}

fn italic_synth(h: &mut Harness) {
    page(h, text("Synthetic italic").font_family("Space Grotesk").italic().size(28.0));
}

fn mono_tabs(h: &mut Harness) {
    page(h, text("fn main() {\n\tlet x = 1;\n}").font_family("JetBrains Mono").size(16.0));
}

fn serif(h: &mut Harness) {
    page(h, text(LOREM).font_family("Lora").size(16.0));
}

fn colored_runs(h: &mut Harness) {
    page(
        h,
        text_rich(vec![
            span("red ").color(Color::from_rgb8(0xEF, 0x44, 0x44)),
            span("green ").color(Color::from_rgb8(0x10, 0xB9, 0x81)),
            span("blue").color(Color::from_rgb8(0x3B, 0x82, 0xF6)),
        ])
        .size(28.0),
    );
}

fn decorations(h: &mut Harness) {
    page(
        h,
        column(vec![
            text("Underlined text").underline().size(20.0).into_widget(),
            text("Struck-through text").strikethrough().size(20.0).into_widget(),
        ])
        .spacing(12.0),
    );
}

fn ellipsis_maxlines(h: &mut Harness) {
    let long: String = LOREM.repeat(4).chars().take(400).collect();
    page(h, container().width(200.0).child(text(long).max_lines(2).ellipsis()));
}

fn partially_offscreen_top(h: &mut Harness) {
    h.mount_widget(transform(Affine::translate((0.0, -10.0)), text("Partially offscreen").size(24.0)));
}

fn subpixel_positions(h: &mut Harness) {
    let rows: Vec<_> = [10.0, 10.25, 10.5, 10.75]
        .iter()
        .map(|x| transform(Affine::translate((*x, 0.0)), text("Ill1|").size(16.0)).into_widget())
        .collect();
    page(h, column(rows).spacing(4.0));
}

fn tiny_6px(h: &mut Harness) {
    page(h, text(PANGRAM).size(6.0));
}

fn dense_paragraph(h: &mut Harness) {
    let long: String = LOREM.repeat(9).chars().take(2000).collect();
    h.mount_widget(container().padding(EdgeInsets::all(10.0)).width(480.0).child(text(long).size(12.0)));
}

fn sharpness_13px(h: &mut Harness) {
    let block = |bg: Color, fg: Color| {
        let line = |s: &'static str, family: Option<&'static str>, size: f32| {
            let t = text(s).size(size).color(fg);
            match family {
                Some(f) => t.font_family(f).into_widget(),
                None => t.into_widget(),
            }
        };
        container()
            .color(bg)
            .padding(EdgeInsets::all(8.0))
            .child(
                column(vec![
                    line("Explorer  src/main.rs  Cargo.toml  README.md", None, 13.0),
                    line("The quick brown fox jumps over the lazy dog 0123", None, 13.0),
                    line("fn main() { let x: u32 = 42; println!(\"{x}\"); }", Some("JetBrains Mono"), 13.0),
                    line("impl<T> Iterator for Il1|O0 { type Item = T; }", Some("JetBrains Mono"), 14.0),
                ])
                .spacing(4.0),
            )
            .into_widget()
    };
    h.mount_widget(column(vec![
        block(Color::from_rgb8(0x1E, 0x1E, 0x1E), Color::from_rgb8(0xD4, 0xD4, 0xD4)),
        block(Color::WHITE, Color::from_rgb8(0x24, 0x29, 0x2E)),
    ]));
}

#[allow(clippy::approx_constant)] // 0.5236 is the §7.2 construction verbatim, not FRAC_PI_6
fn rotated_30(h: &mut Harness) {
    // A fixed 200×60 box well inside the canvas: the transform rotates about its center.
    let boxed = container().width(200.0).height(60.0).child(text("Rotated").size(32.0));
    h.mount_widget(
        container().padding(EdgeInsets::all(120.0)).child(transform(Affine::rotate(0.5236), boxed)),
    );
}

fn scaled_transform_3x(h: &mut Harness) {
    page(h, transform(Affine::scale(3.0), text("Scaled").size(12.0)));
}

fn selection_caret(h: &mut Harness) {
    page(h, text_field().value("Hello world"));
    h.press_tab();
}

/// Bundled Inter glyph ids 36..=60 at 12/24/48 px with no shaping (P2 is independent of
/// Track T).
fn glyph_run_synthetic(p: &mut Painter<'_>) {
    let font =
        peniko::FontData::new(peniko::Blob::from(pebbles_render::fonts::builtin_fonts()[0].1.to_vec()), 0);
    let ink = peniko::Brush::Solid(peniko::Color::from_rgb8(17, 24, 39));
    for (row, size) in [12.0f32, 24.0, 48.0].into_iter().enumerate() {
        let y = 40.0 + 90.0 * row as f32;
        let glyphs = (36u32..=60).enumerate().map(|(i, id)| pebbles_render::paint::Glyph {
            id,
            x: 10.0 + i as f32 * size * 0.62,
            y,
        });
        p.draw_glyphs(&font, size, &[], &ink, Affine::IDENTITY, None, peniko::Fill::NonZero, glyphs);
    }
}

fn two_fonts_same_glyph_ids(p: &mut Painter<'_>) {
    let fonts = pebbles_render::fonts::builtin_fonts();
    let ink = peniko::Brush::Solid(peniko::Color::from_rgb8(17, 24, 39));
    // Inter, JetBrains Mono, Lora (builtin order); whole-pixel pens so every row shares the bins.
    for (row, i) in [0usize, 2, 5].into_iter().enumerate() {
        let Some((_, bytes)) = fonts.get(i) else { continue };
        let font = peniko::FontData::new(peniko::Blob::from(bytes.to_vec()), 0);
        let y = 50.0 + 80.0 * row as f32;
        let glyphs = (36u32..=56).enumerate().map(|(k, id)| pebbles_render::paint::Glyph {
            id,
            x: 10.0 + k as f32 * 16.0,
            y,
        });
        p.draw_glyphs(&font, 24.0, &[], &ink, Affine::IDENTITY, None, peniko::Fill::NonZero, glyphs);
    }
}

fn shadow_blur(p: &mut Painter<'_>) {
    let font = pebbles_render::paint::peniko::FontData::new(
        peniko::Blob::from(pebbles_render::fonts::builtin_fonts()[0].1.to_vec()),
        0,
    );
    let f = swash::FontRef::from_index(font.data.data(), 0).expect("Inter");
    let glyphs: Vec<pebbles_render::paint::Glyph> = "Shadow"
        .chars()
        .enumerate()
        .map(|(i, c)| pebbles_render::paint::Glyph {
            id: u32::from(f.charmap().map(c)),
            x: 16.0 + 38.0 * i as f32,
            y: 64.0,
        })
        .collect();
    let shadow = Color::from_rgba8(0, 0, 0, 0x80);
    p.draw_glyphs_blurred(
        &font,
        48.0,
        &[],
        Affine::translate((3.0, 3.0)),
        None,
        glyphs.iter().copied(),
        shadow,
        3.0,
    );
    let brush = peniko::Brush::Solid(Color::from_rgba8(0x1F, 0x29, 0x37, 0xFF));
    p.draw_glyphs(
        &font,
        48.0,
        &[],
        &brush,
        Affine::IDENTITY,
        None,
        peniko::Fill::NonZero,
        glyphs.into_iter(),
    );
}

fn colr_font() -> peniko::FontData {
    let bytes = include_bytes!("../../fonts/colr-test.ttf");
    peniko::FontData::new(peniko::Blob::from(bytes.to_vec()), 0)
}

/// COLR test glyph `ch` at 100 px, pen (30, 130), under `t`, in the run color `color`.
fn colr_glyph(p: &mut Painter<'_>, ch: char, t: Affine, color: Color) {
    let font = colr_font();
    let id = u32::from(swash::FontRef::from_index(font.data.data(), 0).map_or(0, |f| f.charmap().map(ch)));
    let g = pebbles_render::paint::Glyph { id, x: 30.0, y: 130.0 };
    let brush = peniko::Brush::Solid(color);
    p.draw_glyphs(&font, 100.0, &[], &brush, t, None, peniko::Fill::NonZero, std::iter::once(g));
}

fn colr_v0_layers(p: &mut Painter<'_>) {
    colr_glyph(p, 'A', Affine::IDENTITY, Color::BLACK);
}

fn colr_v0_rotated(p: &mut Painter<'_>) {
    colr_glyph(p, 'A', Affine::rotate_about(0.5, kurbo::Point::new(80.0, 80.0)), Color::BLACK);
}

fn colr_foreground(p: &mut Painter<'_>) {
    colr_glyph(p, 'D', Affine::IDENTITY, Color::from_rgba8(0x10, 0xB9, 0x81, 0xFF));
}

fn colr_v1_gradient(p: &mut Painter<'_>) {
    colr_glyph(p, 'B', Affine::IDENTITY, Color::BLACK);
}

fn colr_v1_composite(p: &mut Painter<'_>) {
    colr_glyph(p, 'C', Affine::IDENTITY, Color::BLACK);
}

#[cfg(test)]
mod tests {
    use crate::scene::logical_scene;

    /// V21 AC2: building each text case twice yields the same op sequence.
    #[test]
    fn text_cases_are_deterministic() {
        for c in super::cases() {
            let (a, b) = (logical_scene(&c, 0), logical_scene(&c, 0));
            assert_eq!(a.ops().len(), b.ops().len(), "{}", c.name);
            let kinds = |s: &pebbles_render::paint::Scene| {
                s.ops()
                    .iter()
                    .map(|o| format!("{o:?}").split([' ', '{']).next().unwrap_or_default().to_string())
                    .collect::<Vec<_>>()
            };
            assert_eq!(kinds(&a), kinds(&b), "{}", c.name);
        }
    }
}
