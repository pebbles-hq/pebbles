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
