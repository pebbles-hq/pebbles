use kurbo::Affine;
use pebbles_render::paint::{Glyph, Painter, Scene};
use peniko::{Brush, Color, Fill, Mix};

use super::{snap_baseline, subpixel_bin};
use crate::prepare::DrawCmd;
use crate::prepare::test_util::prepare_scene;
use crate::stats::Unsupported;

fn font() -> peniko::FontData {
    crate::test_support::bundled_font()
}

fn run(scene: &mut Scene, size: f32, coords: &[i16], t: Affine, glyphs: Vec<Glyph>) {
    Painter::new(scene).draw_glyphs(
        &font(),
        size,
        coords,
        &Brush::Solid(Color::BLACK),
        t,
        None,
        Fill::NonZero,
        glyphs.into_iter(),
    );
}

fn g(id: u32, x: f32) -> Glyph {
    Glyph { id, x, y: 20.0 }
}

#[test]
fn nan_font_size_skips_run() {
    let mut s = Scene::new();
    run(&mut s, f32::NAN, &[], Affine::IDENTITY, vec![g(40, 0.0)]);
    let (plan, stats) = prepare_scene(&s, 64, 64);
    assert!(plan.glyphs.is_empty());
    assert_eq!(stats.non_finite_skipped, 1);
    assert!(stats.unsupported[Unsupported::NonFinite as usize] > 0);
}

#[test]
fn zero_font_size_skips_run() {
    let mut s = Scene::new();
    run(&mut s, 0.0, &[], Affine::IDENTITY, vec![g(40, 0.0)]);
    let (plan, stats) = prepare_scene(&s, 64, 64);
    assert!(plan.glyphs.is_empty() && plan.cmds.is_empty());
    assert_eq!(stats.non_finite_skipped, 0, "silent");
}

#[test]
fn tiny_ppem_skips_run() {
    let mut s = Scene::new();
    run(&mut s, 0.4, &[], Affine::IDENTITY, vec![g(40, 0.0)]);
    let (plan, _) = prepare_scene(&s, 64, 64);
    assert!(plan.glyphs.is_empty());
}

#[test]
fn glyph_id_above_u16_skipped() {
    let mut s = Scene::new();
    run(&mut s, 16.0, &[], Affine::IDENTITY, vec![g(70_000, 0.0), g(40, 10.0)]);
    let (plan, stats) = prepare_scene(&s, 64, 64);
    assert_eq!(plan.glyphs.len(), 1);
    assert_eq!(stats.unsupported[Unsupported::GlyphIdRange as usize], 1);
}

#[test]
fn nan_pen_skips_glyph_only() {
    let mut s = Scene::new();
    run(&mut s, 16.0, &[], Affine::IDENTITY, vec![g(40, f32::NAN), g(41, 10.0), g(42, 20.0)]);
    let (plan, stats) = prepare_scene(&s, 64, 64);
    assert_eq!(plan.glyphs.len(), 2, "the other glyphs still draw");
    assert_eq!(stats.non_finite_skipped, 1);
}

#[test]
fn out_of_range_coords_skip_run() {
    let mut s = Scene::new();
    run(&mut s, 16.0, &[20_000], Affine::IDENTITY, vec![g(40, 0.0)]);
    let (plan, stats) = prepare_scene(&s, 64, 64);
    assert!(plan.glyphs.is_empty());
    assert_eq!(stats.non_finite_skipped, 1);
}

#[test]
fn negative_baseline_rounds_down() {
    assert_eq!(snap_baseline(-0.7), Some(-1));
}

#[test]
fn bin_carries_to_next_pixel() {
    assert_eq!(subpixel_bin(0.9), Some((1, 0)));
    assert_eq!(subpixel_bin(-0.3), Some((-1, 3)));
}

// ---- R25 ----

#[test]
fn offscreen_glyphs_are_not_rasterized() {
    // 10 000 glyphs 20 px apart along x; only those near the 200-px-wide target are kept.
    let mut s = Scene::new();
    run(&mut s, 16.0, &[], Affine::IDENTITY, (0..10_000).map(|i| g(36 + i % 26, i as f32 * 20.0)).collect());
    let (plan, _) = prepare_scene(&s, 200, 100);
    assert!(plan.glyphs.len() <= 100 + 26, "{} requests reach the atlas", plan.glyphs.len());
    assert!(plan.stats.culled_glyphs >= 9_800, "culled {}", plan.stats.culled_glyphs);
}

#[test]
fn edge_glyph_is_kept() {
    // Pen 1 px left of the target: its ink still overlaps the target.
    let mut s = Scene::new();
    run(&mut s, 16.0, &[], Affine::IDENTITY, vec![g(40, -1.0)]);
    let (plan, _) = prepare_scene(&s, 64, 64);
    assert_eq!(plan.glyphs.len(), 1);
    assert_eq!(plan.stats.culled_glyphs, 0);
}

#[test]
fn culling_keeps_order() {
    let mut s = Scene::new();
    {
        let mut p = Painter::new(&mut s);
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &kurbo::Rect::new(0.0, 0.0, 4.0, 4.0));
        p.push_layer(
            Fill::NonZero,
            Mix::Normal,
            1.0,
            Affine::IDENTITY,
            &kurbo::Rect::new(0.0, 0.0, 30.0, 64.0),
        );
    }
    run(&mut s, 16.0, &[], Affine::IDENTITY, vec![g(40, 0.0), g(41, 200.0)]);
    {
        let mut p = Painter::new(&mut s);
        p.pop_layer();
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &kurbo::Rect::new(60.0, 0.0, 64.0, 4.0));
    }
    let (plan, _) = prepare_scene(&s, 64, 64);
    let kinds: Vec<bool> = plan.cmds.iter().map(|c| matches!(c, DrawCmd::Glyphs { .. })).collect();
    assert_eq!(kinds, [false, true, false]);
    assert_eq!(plan.glyphs.len(), 1, "the glyph outside the clip is culled");
}

/// R27 AC1: 02 §7.8 — hinted at 1× / 16 px; never at 2× or 64 px.
#[test]
fn hint_policy_matrix() {
    use super::hint_policy;
    assert!(hint_policy(1.0, 16.0));
    assert!(hint_policy(1.75, 48.0));
    assert!(!hint_policy(2.0, 16.0));
    assert!(!hint_policy(1.0, 64.0));
}

/// R27: atlas-route keys carry the policy's hint flag; the 2× root transform turns it off.
#[test]
fn atlas_keys_follow_hint_policy() {
    let key_hint = |scale: f64| {
        let mut s = Scene::new();
        run(&mut s, 16.0, &[], Affine::scale(scale), vec![g(40, 0.0)]);
        let input = crate::prepare::PrepareInput {
            target_size: (128, 128),
            root_transform: Affine::IDENTITY,
            vertex_limit: u32::MAX,
            scale_factor: scale,
            glyph_hint: None,
        };
        let (plan, _) = crate::prepare::test_util::prepare_with(&s, &input);
        match plan.glyphs[0].key {
            crate::execute::resources::atlas_alloc::CoverageKey::Glyph(k) => k.hint,
            crate::execute::resources::atlas_alloc::CoverageKey::Path(_) => panic!("glyph expected"),
        }
    };
    assert!(key_hint(1.0));
    assert!(!key_hint(2.0));
}
