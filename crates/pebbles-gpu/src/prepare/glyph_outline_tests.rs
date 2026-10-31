use kurbo::{Affine, BezPath, Rect};
use pebbles_render::paint::{Glyph, Painter, Scene};
use peniko::{Color, Fill};

use super::{GlyphRoute, OutlineSource, classify_run};
use crate::prepare::DrawCmd;
use crate::prepare::test_util::prepare_scene;

fn font() -> peniko::FontData {
    crate::test_support::bundled_font()
}

fn glyph(ch: char) -> u16 {
    let f = font();
    swash::FontRef::from_index(f.data.data(), 0).expect("Inter").charmap().map(ch)
}

#[test]
fn classify_identity_is_atlas() {
    assert_eq!(classify_run(Affine::IDENTITY, None, 16.0), GlyphRoute::Atlas { scale: 1.0 });
}

#[test]
fn classify_scale2_is_atlas() {
    assert_eq!(classify_run(Affine::scale(2.0), None, 16.0), GlyphRoute::Atlas { scale: 2.0 });
}

#[test]
fn classify_rotate_is_outline() {
    assert_eq!(classify_run(Affine::rotate(0.3), None, 16.0), GlyphRoute::Outline);
}

#[test]
fn classify_nonuniform_is_outline() {
    assert_eq!(classify_run(Affine::scale_non_uniform(2.0, 1.0), None, 16.0), GlyphRoute::Outline);
}

#[test]
fn classify_mirror_is_outline() {
    assert_eq!(classify_run(Affine::scale_non_uniform(-1.0, 1.0), None, 16.0), GlyphRoute::Outline);
}

#[test]
fn classify_glyph_transform_is_outline() {
    assert_eq!(classify_run(Affine::IDENTITY, Some(Affine::skew(0.25, 0.0)), 16.0), GlyphRoute::Outline);
    assert_eq!(
        classify_run(Affine::IDENTITY, Some(Affine::IDENTITY), 16.0),
        GlyphRoute::Atlas { scale: 1.0 }
    );
}

#[test]
fn classify_huge_ppem_is_outline() {
    assert_eq!(classify_run(Affine::IDENTITY, None, 300.0), GlyphRoute::Outline);
    assert_eq!(classify_run(Affine::IDENTITY, None, f32::NAN), GlyphRoute::Skip);
}

fn outline_of(ch: char, size: f32) -> BezPath {
    let mut p = BezPath::new();
    assert!(OutlineSource::default().outline(&font(), glyph(ch), size, &[], &mut p));
    p
}

#[test]
fn outline_t_is_upright() {
    // Device = translate(pen) · FLIP_Y · outline: the crossbar (font y high) ends up above the
    // stem bottom (font y = 0) — smaller device y.
    let t = Affine::translate((10.0, 50.0)) * Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, 0.0]);
    let device = t * outline_of('T', 32.0);
    let ys: Vec<f64> = device.elements().iter().filter_map(|e| e.end_point()).map(|p| p.y).collect();
    let (top, bottom) =
        (ys.iter().copied().fold(f64::MAX, f64::min), ys.iter().copied().fold(f64::MIN, f64::max));
    assert!(top < bottom && (bottom - 50.0).abs() < 0.5, "top {top} bottom {bottom}");
}

#[test]
fn outline_run_keeps_its_position() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &Rect::new(0.0, 0.0, 4.0, 4.0));
    let g = [Glyph { id: u32::from(glyph('T')), x: 0.0, y: 0.0 }];
    let brush = peniko::Brush::Solid(Color::BLACK);
    p.draw_glyphs(
        &font(),
        24.0,
        &[],
        &brush,
        Affine::translate((30.0, 40.0)) * Affine::rotate(0.4),
        None,
        Fill::NonZero,
        g.into_iter(),
    );
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &Rect::new(60.0, 0.0, 64.0, 4.0));
    let (plan, _) = prepare_scene(&scene, 64, 64);
    // Shape, the glyph's path mask (a coverage request), shape — in painter's order.
    let kinds: Vec<_> = plan.cmds.iter().map(|c| matches!(c, DrawCmd::Glyphs { .. })).collect();
    assert_eq!(kinds, [false, true, false], "{:?}", plan.cmds.len());
    assert!(matches!(plan.glyphs[0].key, crate::execute::resources::atlas_alloc::CoverageKey::Path(_)));
}

#[test]
fn skew_convention_matches_display_list_contract() {
    // G = skew(0.25, 0): a font point at y_up = 100 lands at x = pen.x − 25 (§7.6).
    let pen = kurbo::Point::new(50.0, 200.0);
    let t = Affine::translate(pen.to_vec2())
        * Affine::skew(0.25, 0.0)
        * Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, 0.0]);
    let p = t * kurbo::Point::new(0.0, 100.0);
    assert!((p.x - (pen.x - 25.0)).abs() < 0.01, "x {}", p.x);
    assert!((p.y - (pen.y - 100.0)).abs() < 0.01, "y {}", p.y);
}

// ---- R21: the bounded outline cache ----

fn rotated_run(scene: &mut Scene, ids: &[u16], size: f32) {
    let glyphs: Vec<Glyph> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| Glyph { id: u32::from(*id), x: i as f32 * 10.0, y: 0.0 })
        .collect();
    let brush = peniko::Brush::Solid(Color::BLACK);
    let t = Affine::translate((30.0, 30.0)) * Affine::rotate(0.3);
    Painter::new(scene).draw_glyphs(&font(), size, &[], &brush, t, None, Fill::NonZero, glyphs.into_iter());
}

#[test]
fn outline_cached_across_frames() {
    let mut scratch = crate::prepare::PrepareScratch::default();
    let mut scene = Scene::new();
    rotated_run(&mut scene, &[glyph('A'), glyph('B')], 20.0);
    let input = crate::prepare::PrepareInput {
        target_size: (64, 64),
        root_transform: Affine::IDENTITY,
        vertex_limit: u32::MAX,
        scale_factor: 1.0,
        glyph_hint: None,
        lcd: None,
    };
    let limits = crate::test_support::webgl2_limits();
    let mut stats = crate::stats::FrameStats::default();
    crate::prepare::prepare(&scene, &input, &limits, &mut scratch, &mut stats);
    let first = scratch.outlines.extracted;
    assert!(first >= 1);
    crate::prepare::prepare(&scene, &input, &limits, &mut scratch, &mut stats);
    assert_eq!(scratch.outlines.extracted, first, "frame 2 extracts nothing");
}

#[test]
fn outline_cache_is_bounded() {
    let mut src = OutlineSource::default();
    let f = font();
    for frame in 0..2u32 {
        for i in 0..2500u32 {
            let size = 8.0 + (frame * 2500 + i) as f32 / 64.0;
            let _ = src.cached(&f, glyph('A'), size, &[]);
        }
        src.end_frame();
    }
    assert!(src.census() <= super::GLYPH_OUTLINE_CACHE_MAX, "{}", src.census());
}

#[test]
fn outlines_used_this_frame_survive() {
    let mut src = OutlineSource::default();
    let f = font();
    // Frame 0: 5000 distinct entries, all used this frame → none dropped at its end.
    for i in 0..5000u32 {
        let _ = src.cached(&f, glyph('A'), 8.0 + i as f32 / 64.0, &[]);
    }
    src.end_frame();
    assert_eq!(src.census(), 5000);
    // Frame 1 uses one of them; the cap drops only stale entries.
    let _ = src.cached(&f, glyph('A'), 8.0, &[]);
    src.end_frame();
    assert_eq!(src.census(), super::GLYPH_OUTLINE_CACHE_MAX);
    let before = src.extracted;
    let _ = src.cached(&f, glyph('A'), 8.0, &[]);
    assert_eq!(src.extracted, before, "the entry used in frame 1 survived");
}
