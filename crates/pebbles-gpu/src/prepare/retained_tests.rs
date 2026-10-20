//! M29: retained prepare replays unchanged fragments exactly.

use kurbo::{Affine, Rect, RoundedRect, Stroke};
use pebbles_render::paint::{Glyph, Painter, Scene};
use peniko::{Brush, Color, Fill};

use super::{FramePlan, PrepareInput, PrepareScratch, prepare};
use crate::stats::FrameStats;

/// A row-like fragment: rects, a rounded card, a stroke, a shadow, a star mesh and text.
fn content(seed: u32) -> Scene {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    let font = crate::test_support::bundled_font();
    let c = Color::from_rgb8(u8::try_from(seed % 200).unwrap_or(0), 120, 200);
    p.draw_blurred_rounded_rect(
        Affine::IDENTITY,
        Rect::new(2.0, 4.0, 180.0, 28.0),
        Color::from_rgba8(0, 0, 0, 60),
        6.0,
        3.0,
    );
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::WHITE,
        None,
        &RoundedRect::new(0.0, 0.0, 180.0, 26.0, 6.0),
    );
    p.stroke(
        &Stroke::new(1.0),
        Affine::IDENTITY,
        Color::from_rgb8(200, 200, 210),
        None,
        &RoundedRect::new(0.0, 0.0, 180.0, 26.0, 6.0),
    );
    for k in 0..4 {
        let x = 120.0 + f64::from(k) * 12.0;
        p.fill(Fill::NonZero, Affine::IDENTITY, c, None, &Rect::new(x, 8.0, x + 8.0, 18.0));
    }
    let mut star = kurbo::BezPath::new();
    for k in 0..10 {
        let r = if k % 2 == 0 { 9.0 } else { 4.0 };
        let a = std::f64::consts::PI * f64::from(k) / 5.0;
        let pt = (12.0 + r * a.sin(), 13.0 - r * a.cos());
        if k == 0 { star.move_to(pt) } else { star.line_to(pt) }
    }
    star.close_path();
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgb8(250, 180, 30), None, &star);
    let glyphs: Vec<Glyph> =
        (0..8).map(|k| Glyph { id: 36 + k + seed % 5, x: k as f32 * 8.0, y: 0.0 }).collect();
    p.draw_glyphs(
        &font,
        13.0,
        &[],
        &Brush::Solid(Color::BLACK),
        Affine::translate((30.0, 18.0)),
        None,
        Fill::NonZero,
        glyphs.into_iter(),
    );
    s
}

fn scene_with(frags: &[(&Scene, Affine)]) -> Scene {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(243, 244, 246),
        None,
        &Rect::new(0.0, 0.0, 400.0, 300.0),
    );
    for (f, xf) in frags {
        p.append(f, Some(*xf));
    }
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(20, 20, 30),
        None,
        &Rect::new(0.0, 290.0, 400.0, 300.0),
    );
    s
}

fn input() -> PrepareInput {
    PrepareInput {
        target_size: (400, 300),
        root_transform: Affine::IDENTITY,
        vertex_limit: u32::MAX,
        scale_factor: 1.0,
        glyph_hint: None,
    }
}

fn scratch(retained: bool) -> PrepareScratch {
    PrepareScratch { retained_budget: if retained { 8 << 20 } else { 0 }, ..PrepareScratch::default() }
}

fn run(s: &mut PrepareScratch, scene: &Scene) -> FrameStats {
    let mut stats = FrameStats::default();
    prepare(scene, &input(), &crate::test_support::webgl2_limits(), s, &mut stats);
    stats
}

/// Byte-for-byte equality of everything the encoder reads.
fn assert_same(a: &FramePlan, b: &FramePlan, what: &str) {
    assert_eq!(a.debug_dump(), b.debug_dump(), "{what}: commands");
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&a.solid_vertices),
        bytemuck::cast_slice::<_, u8>(&b.solid_vertices),
        "{what}: vertices"
    );
    assert_eq!(a.solid_indices, b.solid_indices, "{what}: indices");
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&a.shapes),
        bytemuck::cast_slice::<_, u8>(&b.shapes),
        "{what}: shapes"
    );
    assert_eq!(
        bytemuck::cast_slice::<_, u8>(&a.shadows),
        bytemuck::cast_slice::<_, u8>(&b.shadows),
        "{what}: shadows"
    );
    assert_eq!(a.glyphs, b.glyphs, "{what}: glyphs");
    assert_eq!(a.coords, b.coords, "{what}: coords");
    assert_eq!(a.glyph_runs.len(), b.glyph_runs.len(), "{what}: runs");
    for (x, y) in a.glyph_runs.iter().zip(&b.glyph_runs) {
        assert_eq!(
            (x.font.data.id(), x.coords.clone(), x.size_px.to_bits()),
            (y.font.data.id(), y.coords.clone(), y.size_px.to_bits())
        );
    }
    assert_eq!(a.images.len(), b.images.len(), "{what}: images");
    assert_eq!(a.stats, b.stats, "{what}: prepare stats");
    assert_eq!(a.fallback_features, b.fallback_features, "{what}: features");
}

fn assert_stats(a: &FrameStats, b: &FrameStats, what: &str) {
    assert_eq!(a.unsupported, b.unsupported, "{what}: unsupported");
    assert_eq!(
        (a.fallbacks, a.tier_a_draws, a.tier_b_draws, a.tier_c_draws),
        (b.fallbacks, b.tier_a_draws, b.tier_b_draws, b.tier_c_draws),
        "{what}"
    );
}

#[test]
fn retained_fragment_replays_identically() {
    let frag = content(3);
    let scene =
        scene_with(&[(&frag, Affine::translate((20.0, 30.0))), (&frag, Affine::translate((20.0, 80.0)))]);
    let mut s = scratch(true);
    let first = run(&mut s, &scene);
    // One key: the second placement (same sub-pixel phase) already replays the first.
    assert_eq!(s.retained.len(), 1);
    assert!(
        first.ops_prepared < 2 + 2 * (1 + 8) + 2 * 2,
        "the second placement replayed: {}",
        first.ops_prepared
    );
    let hit = run(&mut s, &scene);
    assert!(
        hit.ops_prepared < first.ops_prepared,
        "the hit frame walks fewer ops: {} vs {}",
        hit.ops_prepared,
        first.ops_prepared
    );
    let mut fresh = scratch(false);
    let scratch_stats = run(&mut fresh, &scene);
    assert_same(&s.plan, &fresh.plan, "hit vs scratch");
    assert_stats(&hit, &scratch_stats, "hit vs scratch");
}

#[test]
fn moved_fragment_offsets_positions() {
    let frag = content(1);
    let mut s = scratch(true);
    run(&mut s, &scene_with(&[(&frag, Affine::translate((20.0, 30.0)))]));
    let moved = scene_with(&[(&frag, Affine::translate((23.0, 35.0)))]);
    let hit = run(&mut s, &moved);
    let mut fresh = scratch(false);
    let scratch_stats = run(&mut fresh, &moved);
    assert!(hit.ops_prepared < scratch_stats.ops_prepared, "replayed, not re-walked");
    assert_same(&s.plan, &fresh.plan, "moved by (3, 5)");
    assert_stats(&hit, &scratch_stats, "moved by (3, 5)");
}

#[test]
fn fractional_move_misses() {
    let frag = content(2);
    let mut s = scratch(true);
    run(&mut s, &scene_with(&[(&frag, Affine::translate((20.0, 30.0)))]));
    let moved = scene_with(&[(&frag, Affine::translate((20.5, 30.0)))]);
    let st = run(&mut s, &moved);
    let mut fresh = scratch(false);
    let full = run(&mut fresh, &moved);
    assert_eq!(st.ops_prepared, full.ops_prepared, "a sub-pixel move re-prepares the fragment");
    assert_same(&s.plan, &fresh.plan, "fractional move");
}

#[test]
fn freed_fragment_never_hits() {
    let mut s = scratch(true);
    {
        let frag = content(4);
        run(&mut s, &scene_with(&[(&frag, Affine::translate((20.0, 30.0)))]));
    }
    // The old op list is gone; a new one (other content) is never served from its entry.
    let frag = content(9);
    let scene = scene_with(&[(&frag, Affine::translate((20.0, 30.0)))]);
    let st = run(&mut s, &scene);
    let mut fresh = scratch(false);
    let full = run(&mut fresh, &scene);
    assert_eq!(st.ops_prepared, full.ops_prepared);
    assert_same(&s.plan, &fresh.plan, "new fragment");
}

#[test]
fn prepare_time_drops_on_static_scroll() {
    // 100 rows × 20 ops (2 000 ops) in a 300 px viewport; each row 30 px tall.
    let rows: Vec<Scene> = (0..100)
        .map(|i| {
            let mut s = Scene::new();
            let mut p = Painter::new(&mut s);
            for k in 0..20u32 {
                let x = f64::from(k) * 19.0;
                p.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    Color::from_rgb8(u8::try_from((i * 2) % 255).unwrap_or(0), 100, 200),
                    None,
                    &Rect::new(x, 2.0, x + 16.0, 26.0),
                );
            }
            s
        })
        .collect();
    let list = |scroll: f64| {
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        for (i, r) in rows.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            p.append(r, Some(Affine::translate((4.0, 2.0 + i as f64 * 30.0 - scroll))));
        }
        s
    };
    let mut s = scratch(true);
    let first = run(&mut s, &list(0.0));
    // Scroll one row (30 px, whole pixels): 9 visible rows replay, 1 row is newly visible.
    let second = run(&mut s, &list(30.0));
    let mut fresh = scratch(false);
    let full = run(&mut fresh, &list(30.0));
    assert_same(&s.plan, &fresh.plan, "scrolled");
    assert_eq!(first.ops_prepared, 100 + 2000, "the first frame walks every op");
    // 100 fragment ops + the newly visible and edge rows (≤ 3 rows × 20 ops).
    assert!(second.ops_prepared <= 100 + 3 * 20, "ops_prepared {}", second.ops_prepared);
    assert!(full.ops_prepared >= 2000);
}
