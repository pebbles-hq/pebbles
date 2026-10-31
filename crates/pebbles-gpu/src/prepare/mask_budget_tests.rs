//! M28: path and clip masks are admitted in display-list order against the frame's coverage
//! budget; over it, a path op tessellates (Tier C) and a row-8 clip becomes a GPU mask.

use kurbo::{Affine, BezPath};
use pebbles_render::paint::{Painter, Scene};
use peniko::{Color, Fill};

use super::{PrepareInput, PrepareScratch, prepare};
use crate::stats::FrameStats;

/// A distinct 20 px star per `i` (different geometry → different mask key).
pub(crate) fn icon(i: u32) -> BezPath {
    let mut p = BezPath::new();
    let r_in = 4.0 + f64::from(i % 50) * 0.05;
    for k in 0..10 {
        let r = if k % 2 == 0 { 10.0 } else { r_in };
        let a = std::f64::consts::PI * f64::from(k) / 5.0;
        let pt = (10.0 + r * a.sin(), 10.0 - r * a.cos());
        if k == 0 { p.move_to(pt) } else { p.line_to(pt) }
    }
    p.close_path();
    p
}

fn run(scene: &Scene, budget: u64) -> (super::FramePlan, FrameStats) {
    let mut scratch = PrepareScratch { path_mask_budget: Some(budget), ..PrepareScratch::default() };
    let mut stats = FrameStats::default();
    let input = PrepareInput {
        target_size: (400, 400),
        root_transform: Affine::IDENTITY,
        vertex_limit: u32::MAX,
        scale_factor: 1.0,
        glyph_hint: None,
        lcd: None,
    };
    prepare(scene, &input, &crate::test_support::webgl2_limits(), &mut scratch, &mut stats);
    (scratch.plan, stats)
}

fn icons(n: u32, repeat_first: bool) -> Scene {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    for i in 0..n {
        let g = if repeat_first { icon(0) } else { icon(i) };
        p.fill(
            Fill::NonZero,
            Affine::translate((f64::from(i % 16) * 24.0, f64::from(i / 16) * 24.0)),
            Color::BLACK,
            None,
            &g,
        );
    }
    s
}

#[test]
fn masks_over_budget_fall_to_tier_c_in_order() {
    let one = super::path_masks::budget_area(
        &pebbles_render::paint::display_list::Geometry::Path(icon(0)),
        Affine::IDENTITY,
        None,
    );
    let (plan, stats) = run(&icons(10, false), 3 * one + one / 2);
    assert_eq!(stats.tier_b_draws, 3, "the first three icons fit");
    assert_eq!(plan.stats.path_mask_budget_fallbacks, 7);
    // Display-list order: the three masks are the first three ops.
    let kinds = crate::prepare::test_util::cmd_kinds(&plan);
    assert_eq!(&kinds[..1], ["glyphs"]);
    assert!(kinds[1..].iter().all(|k| *k == "triangles"), "{kinds:?}");
}

#[test]
fn a_repeated_mask_is_admitted_once() {
    let one = super::path_masks::budget_area(
        &pebbles_render::paint::display_list::Geometry::Path(icon(0)),
        Affine::IDENTITY,
        None,
    );
    // The same geometry at integer offsets shares one key: it costs the budget once.
    let (plan, stats) = run(&icons(10, true), one + 1);
    assert_eq!(stats.tier_b_draws, 10);
    assert_eq!(plan.stats.path_mask_budget_fallbacks, 0);
}

#[test]
fn clip_over_budget_becomes_a_gpu_mask() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    p.push_layer(Fill::NonZero, peniko::Mix::Normal, 1.0, Affine::IDENTITY, &icon(1));
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &kurbo::Rect::new(0.0, 0.0, 20.0, 20.0));
    p.pop_layer();
    let (plan, _) = run(&s, 8 << 20);
    assert_eq!((plan.stats.clips_mask, plan.stats.clips_gpu_mask), (1, 0), "fits: a CPU mask (row 8)");
    let (plan, _) = run(&s, 16);
    assert_eq!((plan.stats.clips_mask, plan.stats.clips_gpu_mask), (0, 1), "over budget: row 9");
}
