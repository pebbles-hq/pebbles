//! The bench scenes of 10 §11.1, shared by `render-check bench` (M21) and the device-free
//! microbench (M25). Deterministic: no clocks, no system fonts (bundled Inter only, DR-15).

use kurbo::{Affine, BezPath, Circle, Point, Rect, RoundedRect, Stroke};
use pebbles_render::paint::{Glyph, Painter};
use peniko::{BlendMode, Brush, Color, Compose, Fill, Gradient, Mix};

/// A bench builder: draws one case into a painter.
pub type BenchFn = fn(&mut Painter<'_>);

/// `(name, canvas, builder)` of every §11.1 case.
pub const BENCH_CASES: &[(&str, (u32, u32), BenchFn)] = &[
    ("bench/dashboard_1200x800", (1200, 800), dashboard),
    ("bench/rects_10k", (1000, 1000), rects_10k),
    ("bench/rrects_2k", (1000, 1000), rrects_2k),
    ("bench/paths_2k", (1000, 1000), paths_2k),
    ("bench/glyph_runs_1k", (1200, 1000), glyph_runs_1k),
    ("bench/glyph_cold", (1200, 1000), glyph_runs_1k),
    ("bench/images_64", (1024, 1024), images_64),
    ("bench/shadows_200", (1200, 800), shadows_200),
    ("bench/layers_100", (1200, 800), layers_100),
    ("bench/gradients_500", (1200, 800), gradients_500),
    ("bench/static_list_scroll", (800, 1000), static_list_scroll),
];

/// Cases that run on a fresh renderer every iteration (cold caches).
pub const COLD_CASES: &[&str] = &["bench/glyph_cold"];

const ID: Affine = Affine::IDENTITY;

fn fill(p: &mut Painter<'_>, brush: impl Into<Brush>, shape: &impl kurbo::Shape) {
    p.fill(Fill::NonZero, ID, &brush.into(), None, shape);
}

fn card_rects() -> impl Iterator<Item = Rect> {
    let (cols, rows) = (3u8, 2u8);
    let (mx, my, gap, w) = (48.0, 260.0, 32.0, 1200.0);
    let cw = (w - 2.0 * mx - f64::from(cols - 1) * gap) / f64::from(cols);
    (0..rows).flat_map(move |r| {
        (0..cols).map(move |c| {
            let (x, y) = (mx + f64::from(c) * (cw + gap), my + f64::from(r) * (150.0 + gap));
            Rect::new(x, y, x + cw, y + 150.0)
        })
    })
}

fn linear(p0: (f64, f64), p1: (f64, f64), a: Color, b: Color) -> Gradient {
    Gradient::new_linear(Point::from(p0), Point::from(p1)).with_stops([a, b].as_slice())
}

/// The dashboard scene of 10 §10.5.
pub fn dashboard(p: &mut Painter<'_>) {
    let hero =
        linear((0.0, 0.0), (1200.0, 220.0), Color::from_rgb8(99, 102, 241), Color::from_rgb8(236, 72, 153));
    let accent =
        linear((0.0, 0.0), (1200.0, 0.0), Color::from_rgb8(16, 185, 129), Color::from_rgb8(59, 130, 246));
    fill(p, Color::from_rgb8(15, 17, 26), &Rect::new(0.0, 0.0, 1200.0, 800.0));
    fill(p, hero, &Rect::new(0.0, 0.0, 1200.0, 220.0));
    for r in card_rects() {
        let card = RoundedRect::from_rect(r, 16.0);
        p.draw_blurred_rounded_rect(ID, r, Color::from_rgba8(0, 0, 0, 90), 16.0, 14.0);
        fill(p, Color::from_rgb8(28, 31, 44), &card);
        p.push_layer(Fill::NonZero, Mix::Normal, 1.0, ID, &card);
        fill(p, accent.clone(), &Rect::new(r.x0, r.y0, r.x1, r.y0 + 40.0));
        p.pop_layer();
        p.stroke(&Stroke::new(1.0), ID, Color::from_rgba8(255, 255, 255, 24), None, &card);
    }
    // ShaderMask (DestIn): a magenta panel faded out to the right.
    let mb = Rect::new(48.0, 620.0, 560.0, 680.0);
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, ID, &mb);
    fill(p, Color::from_rgb8(236, 72, 153), &mb);
    p.push_layer(Fill::NonZero, BlendMode::new(Mix::Normal, Compose::DestIn), 1.0, ID, &mb);
    let fade = linear((mb.x0, 0.0), (mb.x1, 0.0), Color::WHITE, Color::from_rgba8(255, 255, 255, 0));
    fill(p, fade, &mb);
    p.pop_layer();
    p.pop_layer();
    // Translucent bottom action bar.
    let bar = Rect::new(48.0, 700.0, 1152.0, 760.0);
    p.push_layer(Fill::NonZero, Mix::Normal, 0.85, ID, &bar);
    fill(p, accent, &bar);
    p.pop_layer();
}

/// 100×100 grid of 8×8 rects, two alternating colors.
pub fn rects_10k(p: &mut Painter<'_>) {
    let (a, b) = (Color::from_rgb8(59, 130, 246), Color::from_rgb8(239, 68, 68));
    for i in 0..100u16 {
        for j in 0..100u16 {
            let (x, y) = (f64::from(i) * 10.0, f64::from(j) * 10.0);
            fill(p, if (i + j) % 2 == 0 { a } else { b }, &Rect::new(x, y, x + 8.0, y + 8.0));
        }
    }
}

/// 2 000 rounded rects (radius 6) with 1 px strokes.
pub fn rrects_2k(p: &mut Painter<'_>) {
    for i in 0..2000u16 {
        let (x, y) = (f64::from(i % 50) * 20.0, f64::from(i / 50) * 25.0);
        let r = RoundedRect::new(x + 1.0, y + 1.0, x + 18.0, y + 22.0, 6.0);
        fill(p, Color::from_rgb8(229, 231, 235), &r);
        p.stroke(&Stroke::new(1.0), ID, Color::from_rgb8(75, 85, 99), None, &r);
    }
}

fn icon() -> BezPath {
    let mut b = BezPath::new();
    b.move_to((12.0, 2.0));
    b.curve_to((18.0, 2.0), (22.0, 6.0), (22.0, 12.0));
    b.curve_to((22.0, 18.0), (18.0, 22.0), (12.0, 22.0));
    b.curve_to((6.0, 22.0), (2.0, 18.0), (2.0, 12.0));
    b.curve_to((2.0, 7.0), (6.0, 3.0), (12.0, 2.0));
    b.close_path();
    b
}

/// 2 000 copies of one 24 px icon, filled and stroked (1.5 px), translation-only.
pub fn paths_2k(p: &mut Painter<'_>) {
    let ic = icon();
    for i in 0..2000u16 {
        let xf = Affine::translate((f64::from(i % 40) * 25.0, f64::from(i / 40) * 20.0));
        p.fill(Fill::NonZero, xf, Color::from_rgb8(16, 185, 129), None, &ic);
        p.stroke(&Stroke::new(1.5), xf, Color::from_rgb8(6, 95, 70), None, &ic);
    }
}

/// `text` shaped with the bundled Inter charmap and advances at `size` px (deterministic,
/// DR-15: no system fonts, no shaping engine).
fn shape(font: &peniko::FontData, text: &str, size: f32) -> Vec<Glyph> {
    use skrifa::MetadataProvider as _;
    let Ok(f) = skrifa::FontRef::from_index(font.data.data(), font.index) else { return Vec::new() };
    let (cmap, metrics) = (
        f.charmap(),
        f.glyph_metrics(skrifa::instance::Size::new(size), skrifa::instance::LocationRef::default()),
    );
    let mut x = 0.0;
    text.chars()
        .filter_map(|c| cmap.map(c))
        .map(|g| {
            let glyph = Glyph { id: g.to_u32(), x, y: 0.0 };
            x += metrics.advance_width(g).unwrap_or(0.0);
            glyph
        })
        .collect()
}

/// 1 000 runs × 40 glyphs of bundled Inter at 14 px.
pub fn glyph_runs_1k(p: &mut Painter<'_>) {
    let font =
        peniko::FontData::new(peniko::Blob::from(pebbles_render::fonts::builtin_fonts()[0].1.to_vec()), 0);
    let line = "The quick brown fox jumps over the lazy";
    let glyphs = shape(&font, line, 14.0);
    let brush = Brush::Solid(Color::from_rgb8(17, 24, 39));
    for i in 0..1000u16 {
        let xf = Affine::translate((8.0 + f64::from(i % 4) * 296.0, 16.0 + f64::from(i / 4) * 3.9));
        p.draw_glyphs(&font, 14.0, &[], &brush, xf, None, Fill::NonZero, glyphs.iter().copied());
    }
}

/// 64 distinct 128×128 generated images in an 8×8 grid.
pub fn images_64(p: &mut Painter<'_>) {
    for i in 0..64u32 {
        let px: Vec<u8> = (0..128 * 128u32)
            .flat_map(|k| {
                let (x, y) = (k % 128, k / 128);
                [(x * 2 + i * 3).to_le_bytes()[0], (y * 2).to_le_bytes()[0], (i * 4).to_le_bytes()[0], 255]
            })
            .collect();
        let img = peniko::ImageBrush::new(peniko::ImageData {
            data: peniko::Blob::from(px),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::Alpha,
            width: 128,
            height: 128,
        });
        p.draw_image(&img, Affine::translate((f64::from(i % 8) * 128.0, f64::from(i / 8) * 128.0)));
    }
}

/// 200 elevation shadows.
pub fn shadows_200(p: &mut Painter<'_>) {
    fill(p, Color::from_rgb8(243, 244, 246), &Rect::new(0.0, 0.0, 1200.0, 800.0));
    for i in 0..200u16 {
        let (x, y) = (20.0 + f64::from(i % 20) * 58.0, 20.0 + f64::from(i / 20) * 78.0);
        let r = Rect::new(x, y, x + 44.0, y + 60.0);
        p.draw_blurred_rounded_rect(
            ID,
            r.with_origin((x, y + 4.0)),
            Color::from_rgba8(0, 0, 0, 60),
            8.0,
            6.0,
        );
        fill(p, Color::WHITE, &RoundedRect::from_rect(r, 8.0));
    }
}

/// 100 opacity groups.
pub fn layers_100(p: &mut Painter<'_>) {
    for i in 0..100u16 {
        let (x, y) = (f64::from(i % 10) * 118.0, f64::from(i / 10) * 78.0);
        let r = Rect::new(x + 4.0, y + 4.0, x + 112.0, y + 72.0);
        p.push_layer(Fill::NonZero, Mix::Normal, 0.5 + f32::from(i % 5) * 0.1, ID, &r);
        fill(p, Color::from_rgb8(59, 130, 246), &r);
        fill(p, Color::from_rgb8(239, 68, 68), &Circle::new(r.center(), 24.0));
        p.pop_layer();
    }
}

/// A 2 000-op list: 100 rows (each a retained fragment of 20 ops: card, stroke, icon, text
/// stand-in bars) placed at whole-pixel offsets (M29's static scroll).
pub fn static_list_scroll(p: &mut Painter<'_>) {
    for i in 0..100u16 {
        let mut row = pebbles_render::paint::Scene::new();
        {
            let mut r = Painter::new(&mut row);
            let card = RoundedRect::new(0.0, 0.0, 760.0, 34.0, 6.0);
            fill(&mut r, Color::WHITE, &card);
            r.stroke(&Stroke::new(1.0), ID, Color::from_rgb8(229, 231, 235), None, &card);
            r.fill(
                Fill::NonZero,
                Affine::translate((8.0, 7.0)),
                Color::from_rgb8(16, 185, 129),
                None,
                &icon(),
            );
            for k in 0..17u16 {
                let x = 40.0 + f64::from(k) * 42.0;
                let w = 20.0 + f64::from((k * 7 + i) % 18);
                fill(&mut r, Color::from_rgb8(55, 65, 81), &Rect::new(x, 12.0, x + w, 22.0));
            }
        }
        p.append(&row, Some(Affine::translate((20.0, 10.0 + f64::from(i) * 40.0))));
    }
}

/// 500 linear and radial gradient fills.
pub fn gradients_500(p: &mut Painter<'_>) {
    for i in 0..500u16 {
        let (x, y) = (f64::from(i % 25) * 48.0, f64::from(i / 25) * 40.0);
        let r = Rect::new(x + 2.0, y + 2.0, x + 46.0, y + 38.0);
        let (a, b) = (Color::from_rgb8(99, 102, 241), Color::from_rgb8(236, 72, 153));
        let g = if i % 2 == 0 {
            linear((r.x0, r.y0), (r.x1, r.y1), a, b)
        } else {
            Gradient::new_radial(r.center(), 22.0).with_stops([a, b].as_slice())
        };
        fill(p, g, &r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_case_builds_a_non_empty_scene() {
        for (name, _, f) in BENCH_CASES {
            let mut s = pebbles_render::paint::Scene::new();
            f(&mut Painter::new(&mut s));
            assert!(!s.ops().is_empty(), "{name}");
        }
    }

    #[test]
    fn glyph_runs_shape_every_char() {
        let font = peniko::FontData::new(
            peniko::Blob::from(pebbles_render::fonts::builtin_fonts()[0].1.to_vec()),
            0,
        );
        assert_eq!(shape(&font, "The quick brown fox jumps over the lazy", 14.0).len(), 39);
    }
}
