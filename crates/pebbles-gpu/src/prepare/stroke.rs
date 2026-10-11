//! Stroke expansion: outline the path in local space, then transform; hairline rule.

use std::ops::Range;

use kurbo::{Affine, BezPath, Stroke};
use lyon_tessellation::{BuffersBuilder, FillOptions, FillRule, FillTessellator, VertexBuffers};

use super::plan::FramePlan;
use super::tessellate::{SolidVertexCtor, append, to_lyon_path};

/// Stroke-outline expansion tolerance (device px).
pub(crate) const STROKE_TOLERANCE_PX: f64 = 0.1;

/// Reusable stroke-expansion scratch (zero steady-state allocation).
#[derive(Default)]
pub(crate) struct StrokeScratch {
    pub ctx: kurbo::StrokeCtx,
    pub outline: BezPath,
}

/// The result of expanding a stroke in local space.
pub(crate) struct StrokeOutcome {
    /// Fold this factor into the premultiplied brush color (1.0 unless a hairline).
    pub alpha: f32,
    /// Whether the stroke produced geometry (`false` = zero/non-finite width).
    pub drew: bool,
}

/// Expand `path`'s stroke in **local** space and store the device-space outline in
/// `scratch.outline`. Width, dashes, joins and caps are interpreted in the path's own space
/// (02 §6.3); the caller fill-tessellates `scratch.outline` and folds `alpha` into the color.
pub(crate) fn expand(
    path: &BezPath,
    style: &Stroke,
    transform: Affine,
    scratch: &mut StrokeScratch,
) -> StrokeOutcome {
    // Device width = the geometric mean under non-uniform scale (02 §5.5).
    let wd = style.width * transform.determinant().abs().sqrt();
    if !style.width.is_finite() || style.width <= 0.0 {
        return StrokeOutcome { alpha: 1.0, drew: false };
    }
    // Hairline rule: stroke at exactly 1 device px and fold the width into alpha.
    let (local_width, alpha) =
        if wd > 0.0 && wd < 1.0 { (style.width / wd, wd as f32) } else { (style.width, 1.0) };
    let s = transform.spectral_norm().max(1e-6);
    let mut style_adj = style.clone();
    style_adj.width = local_width;
    kurbo::stroke_with(
        path.elements().iter().copied(),
        &style_adj,
        &kurbo::StrokeOpts::default(),
        STROKE_TOLERANCE_PX / s,
        &mut scratch.ctx,
    );
    scratch.outline = scratch.ctx.output().clone();
    scratch.outline.apply_affine(transform);
    StrokeOutcome { alpha, drew: true }
}

/// Fill-tessellate `scratch.outline` (already device-space) with a premultiplied color,
/// appending to the solid arenas and returning the index range.
pub(crate) fn fill_outline(plan: &mut FramePlan, scratch: &StrokeScratch, color: [f32; 4]) -> Range<u32> {
    let lyon = to_lyon_path(&scratch.outline);
    let mut geometry = VertexBuffers::new();
    let mut tess = FillTessellator::new();
    let options = FillOptions::default()
        .with_tolerance(super::tessellate::FILL_TOLERANCE_PX)
        .with_fill_rule(FillRule::NonZero);
    let _ = tess.tessellate_path(
        &lyon,
        &options,
        &mut BuffersBuilder::new(&mut geometry, SolidVertexCtor(color)),
    );
    append(plan, geometry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prepare::color::premul_scaled;
    use kurbo::{Affine, Cap, Point};

    fn path() -> BezPath {
        BezPath::from_vec(vec![
            kurbo::PathEl::MoveTo(Point::new(0.0, 0.0)),
            kurbo::PathEl::LineTo(Point::new(10.0, 0.0)),
        ])
    }

    fn run(style: &Stroke, transform: Affine) -> (FramePlan, StrokeOutcome) {
        let mut plan = FramePlan::default();
        let mut scratch = StrokeScratch::default();
        let outcome = expand(&path(), style, transform, &mut scratch);
        if outcome.drew {
            let color = premul_scaled(peniko::Color::BLACK, outcome.alpha);
            fill_outline(&mut plan, &scratch, color);
        }
        (plan, outcome)
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
        let mut plan = FramePlan::default();
        let mut scratch = StrokeScratch::default();
        let short = BezPath::from_vec(vec![
            kurbo::PathEl::MoveTo(Point::new(0.0, 0.0)),
            kurbo::PathEl::LineTo(Point::new(4.0, 0.0)),
        ]);
        let outcome = expand(&short, &style, Affine::scale(2.0), &mut scratch);
        assert!(outcome.drew);
        fill_outline(&mut plan, &scratch, premul_scaled(peniko::Color::BLACK, outcome.alpha));
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
            lo = lo.min(proj as f32);
            hi = hi.max(proj as f32);
        }
        assert!((hi - lo - 2.0).abs() < 0.05, "width {}", hi - lo);
    }
}
