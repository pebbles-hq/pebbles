//! Stroke expansion tests (C10 local-space + hairline, R05 dash guard, S04 width clamp).

use super::*;
use crate::prepare::color::premul_scaled;
use crate::prepare::plan::FramePlan;
use crate::prepare::tessellate::PathTessellator;
use crate::stats::FrameStats;
use kurbo::{Affine, Cap, Point};

fn path() -> BezPath {
    BezPath::from_vec(vec![PathEl::MoveTo(Point::new(0.0, 0.0)), PathEl::LineTo(Point::new(10.0, 0.0))])
}

fn run_path(path: &BezPath, style: &Stroke, transform: Affine) -> (FramePlan, StrokeOutcome) {
    let mut plan = FramePlan::default();
    let mut tess = PathTessellator::default();
    let outcome = expand(path, style, transform, &mut tess.stroke_ctx);
    if outcome.drew {
        let color = premul_scaled(peniko::Color::BLACK, outcome.alpha);
        tess.fill_stroke_outline(&mut plan, &mut FrameStats::default(), transform, color)
            .expect("tessellates");
    }
    (plan, outcome)
}

fn run(style: &Stroke, transform: Affine) -> (FramePlan, StrokeOutcome) {
    run_path(&path(), style, transform)
}

fn bbox(plan: &FramePlan) -> (f32, f32, f32, f32) {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for v in &plan.solid_vertices {
        min_x = min_x.min(v.pos[0]);
        min_y = min_y.min(v.pos[1]);
        max_x = max_x.max(v.pos[0]);
        max_y = max_y.max(v.pos[1]);
    }
    (min_x, min_y, max_x, max_y)
}

#[test]
fn stroke_width_scales_with_transform() {
    let (plan, outcome) = run(&Stroke::new(1.0), Affine::scale(2.0));
    assert!(outcome.drew);
    let (_, min_y, _, max_y) = bbox(&plan);
    assert!((max_y - min_y - 2.0).abs() < 0.05, "y-extent {}", max_y - min_y);
}

#[test]
fn dash_lengths_scale_with_transform() {
    let style = Stroke::new(1.0).with_caps(Cap::Butt).with_dashes(0.0, [4.0, 4.0]);
    // A line of length 4 under dashes [4,4] yields a single 4-unit dash (butt caps).
    let short =
        BezPath::from_vec(vec![PathEl::MoveTo(Point::new(0.0, 0.0)), PathEl::LineTo(Point::new(4.0, 0.0))]);
    let (plan, outcome) = run_path(&short, &style, Affine::scale(2.0));
    assert!(outcome.drew);
    let (min_x, _, max_x, _) = bbox(&plan);
    assert!((max_x - min_x - 8.0).abs() < 0.1, "first dash x-extent {}", max_x - min_x);
}

#[test]
fn hairline_draws_one_device_px_with_scaled_alpha() {
    let (plan, outcome) = run(&Stroke::new(0.25), Affine::IDENTITY);
    assert!(outcome.drew);
    assert!((outcome.alpha - 0.25).abs() < 1e-6, "alpha {}", outcome.alpha);
    let (_, min_y, _, max_y) = bbox(&plan);
    assert!((max_y - min_y - 1.0).abs() < 0.05, "y-extent {}", max_y - min_y);
}

#[test]
fn zero_width_stroke_draws_nothing() {
    let (plan, outcome) = run(&Stroke::new(0.0), Affine::IDENTITY);
    assert!(!outcome.drew);
    assert!(plan.solid_vertices.is_empty());
}

#[test]
fn rotated_stroke_keeps_width() {
    let (plan, outcome) = run(&Stroke::new(2.0), Affine::rotate(std::f64::consts::FRAC_PI_6));
    assert!(outcome.drew);
    // Perpendicular distance between the outline's parallel edges = the stroke width (2).
    let n = kurbo::Vec2::new(-(std::f64::consts::FRAC_PI_6).sin(), (std::f64::consts::FRAC_PI_6).cos());
    let mut lo = f32::MAX;
    let mut hi = f32::MIN;
    for v in &plan.solid_vertices {
        let proj = (v.pos[0] as f64) * n.x + (v.pos[1] as f64) * n.y;
        lo = lo.min(to_f32(proj).unwrap_or(0.0));
        hi = hi.max(to_f32(proj).unwrap_or(0.0));
    }
    assert!((hi - lo - 2.0).abs() < 0.05, "width {}", hi - lo);
}

// ---- R05: dash guard ----

fn line(len: f64) -> BezPath {
    BezPath::from_vec(vec![PathEl::MoveTo(Point::new(0.0, 10.0)), PathEl::LineTo(Point::new(len, 10.0))])
}

fn dashed(pattern: &[f64]) -> Stroke {
    Stroke::new(2.0).with_caps(Cap::Butt).with_dashes(0.0, pattern.iter().copied())
}

/// Prepare one stroke of `path` through the walk (R02 + dash guard + tessellation).
fn prepare_stroke(style: &Stroke, path: &BezPath) -> (FramePlan, FrameStats) {
    let mut scene = pebbles_render::paint::Scene::new();
    pebbles_render::paint::Painter::new(&mut scene).stroke(
        style,
        Affine::IDENTITY,
        peniko::Color::BLACK,
        None,
        path,
    );
    crate::prepare::test_util::prepare_scene(&scene, 2100, 100)
}

/// Number of subpaths kurbo produced for the last `expand` (one per dash for a straight line).
fn pieces(ctx: &kurbo::StrokeCtx) -> usize {
    ctx.output().elements().iter().filter(|el| matches!(el, PathEl::MoveTo(_))).count()
}

#[test]
fn zero_period_strokes_solid() {
    let style = dashed(&[0.0, 0.0]);
    let degenerate = DashPlan::Solid { alpha: 1.0, kind: Unsupported::DashDegenerate };
    assert_eq!(plan_dashes(&style, &line(100.0), 1.0), degenerate);
    let (plan, stats) = prepare_stroke(&style, &line(100.0));
    assert_eq!(plan.cmds.len(), 1);
    assert_eq!(stats.unsupported[Unsupported::DashDegenerate as usize], 1);
}

#[test]
fn negative_dash_strokes_solid() {
    let style = dashed(&[4.0, -1.0]);
    let degenerate = DashPlan::Solid { alpha: 1.0, kind: Unsupported::DashDegenerate };
    assert_eq!(plan_dashes(&style, &line(100.0), 1.0), degenerate);
    let mut ctx = kurbo::StrokeCtx::default();
    let outcome = expand(&line(100.0), &style, Affine::IDENTITY, &mut ctx);
    assert_eq!(outcome.dash_fallback, Some(Unsupported::DashDegenerate));
    assert_eq!(pieces(&ctx), 1, "solid");
}

#[test]
fn nan_offset_stroke_is_skipped_by_r02() {
    let style = Stroke::new(2.0).with_dashes(f64::NAN, [4.0, 4.0]);
    let (plan, stats) = prepare_stroke(&style, &line(100.0));
    assert!(plan.cmds.is_empty());
    assert_eq!(stats.non_finite_skipped, 1);
    assert_eq!(stats.unsupported[Unsupported::DashDegenerate as usize], 0);
}

#[test]
fn microscopic_dashes_fall_back() {
    let style = dashed(&[1e-6, 1e-6]);
    let start = std::time::Instant::now();
    let plan_kind = plan_dashes(&style, &line(2000.0), 1.0);
    assert_eq!(plan_kind, DashPlan::Solid { alpha: 0.5, kind: Unsupported::DashCountCapped });
    let (plan, stats) = prepare_stroke(&style, &line(2000.0));
    let elapsed = start.elapsed();
    assert_eq!(stats.unsupported[Unsupported::DashCountCapped as usize], 1);
    assert!(plan.solid_vertices.len() < 64, "{} vertices", plan.solid_vertices.len());
    // Brush alpha (1.0) × duty cycle (0.5), premultiplied.
    assert!(plan.solid_vertices.iter().all(|v| (v.color[3] - 0.5).abs() < 1e-6));
    // 02 R05 AC2 budget (release builds; debug timing is not representative).
    #[cfg(not(debug_assertions))]
    assert!(elapsed < std::time::Duration::from_millis(50), "{elapsed:?}");
    let _ = elapsed;
}

#[test]
fn normal_dashes_unchanged() {
    let style = dashed(&[6.0, 3.0]);
    assert_eq!(plan_dashes(&style, &line(100.0), 1.0), DashPlan::Dashed);
    let mut ctx = kurbo::StrokeCtx::default();
    let outcome = expand(&line(100.0), &style, Affine::IDENTITY, &mut ctx);
    assert_eq!(outcome.dash_fallback, None);
    assert!(pieces(&ctx) >= 10, "{} pieces", pieces(&ctx));
    // Identical to kurbo's own dashing of the unmodified style.
    let mut direct = kurbo::StrokeCtx::default();
    let opts = kurbo::StrokeOpts::default();
    kurbo::stroke_with(
        line(100.0).elements().iter().copied(),
        &style,
        &opts,
        STROKE_TOLERANCE_PX,
        &mut direct,
    );
    assert_eq!(ctx.output(), direct.output());
}

#[test]
fn odd_pattern_period_doubles() {
    assert_eq!(dash_period(&[3.0, 2.0, 1.0]), (12.0, 6.0));
    assert_eq!(dash_period(&[1.0, 4.0]), (5.0, 1.0));
    // Odd patterns alternate on/off roles, so the duty cycle is exactly 1/2 (not 5/6).
    let style = dashed(&[1e-6, 1e-6, 4e-6]);
    let capped = DashPlan::Solid { alpha: 0.5, kind: Unsupported::DashCountCapped };
    assert_eq!(plan_dashes(&style, &line(2000.0), 1.0), capped);
    // 200 / 12 ≈ 17 doubled periods, but 200 / 6 would be 33: the period is the doubled one.
    let style = dashed(&[0.03, 0.02, 0.01]);
    assert_eq!(plan_dashes(&style, &line(1000.0), 1.0), DashPlan::Dashed);
    assert_eq!(
        plan_dashes(&style, &line(1500.0), 1.0),
        DashPlan::Solid { alpha: 0.5, kind: Unsupported::DashCountCapped }
    );
}
