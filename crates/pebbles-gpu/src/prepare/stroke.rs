//! Stroke expansion: outline the path in local space, then transform; hairline rule.

use kurbo::{Affine, BezPath, PathEl, Stroke};

use super::num::to_f32;
use crate::stats::Unsupported;

/// Stroke-outline expansion tolerance (device px).
pub(crate) const STROKE_TOLERANCE_PX: f64 = 0.1;

/// Dash expansion bound (02 §6.1): more estimated dashes than this strokes solid.
pub(crate) const MAX_DASH_SEGMENTS: u32 = 10_000;

/// How a stroke's dash pattern is expanded (02 §6.3 dash guard).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DashPlan {
    /// No dash pattern.
    Undashed,
    /// A valid pattern within `MAX_DASH_SEGMENTS`: kurbo dashes it.
    Dashed,
    /// Stroke solid with the brush alpha × `alpha`, recording `kind`.
    Solid { alpha: f32, kind: Unsupported },
}

/// `(period, on)` of a dash pattern: odd patterns are doubled (kurbo `dash_iter`), `on` sums
/// the entries at even indices of the (doubled) pattern.
pub(crate) fn dash_period(pattern: &[f64]) -> (f64, f64) {
    let reps = if pattern.len() % 2 == 1 { 2 } else { 1 };
    let doubled = || pattern.iter().cycle().take(pattern.len() * reps);
    (doubled().sum(), doubled().step_by(2).sum())
}

/// Decide how to dash `style` on `path` under device scale `s` (non-finite input never
/// reaches here: R02 skips such strokes).
pub(crate) fn plan_dashes(style: &Stroke, path: &BezPath, s: f64) -> DashPlan {
    let pattern = &style.dash_pattern[..];
    if pattern.is_empty() {
        return DashPlan::Undashed;
    }
    let (period, on) = dash_period(pattern);
    if pattern.iter().any(|&d| d < 0.0) || period <= 0.0 {
        return DashPlan::Solid { alpha: 1.0, kind: Unsupported::DashDegenerate };
    }
    // Control-polygon length: an upper bound of the arc length.
    let mut length = 0.0;
    let mut last: Option<kurbo::Point> = None;
    for el in path.elements() {
        let pts: &[kurbo::Point] = match el {
            PathEl::MoveTo(p) => {
                last = Some(*p);
                continue;
            }
            PathEl::LineTo(p) => &[*p],
            PathEl::QuadTo(a, p) => &[*a, *p],
            PathEl::CurveTo(a, b, p) => &[*a, *b, *p],
            PathEl::ClosePath => continue,
        };
        for &p in pts {
            if let Some(q) = last {
                length += (p - q).hypot();
            }
            last = Some(p);
        }
    }
    let segments = length * s / (period * s);
    if segments > f64::from(MAX_DASH_SEGMENTS) {
        return DashPlan::Solid {
            alpha: to_f32(on / period).unwrap_or(1.0),
            kind: Unsupported::DashCountCapped,
        };
    }
    DashPlan::Dashed
}

/// (σmax, σmin) of the linear part of `t`.
pub(crate) fn singular_values(t: &Affine) -> (f64, f64) {
    let [a, b, c, d, _, _] = t.as_coeffs();
    let s = a * a + b * b + c * c + d * d;
    let det = a * d - b * c;
    let root = (s * s - 4.0 * det * det).max(0.0).sqrt();
    (((s + root) / 2.0).sqrt(), ((s - root) / 2.0).max(0.0).sqrt())
}

/// Largest LOCAL half-width that can still change coverage inside `target` (11 §2.4).
pub(crate) fn max_useful_half_width(path: &BezPath, t: &Affine, target: kurbo::Rect) -> Option<f64> {
    let first = path.elements().iter().find_map(|el| match *el {
        PathEl::MoveTo(p) => Some(p),
        _ => None,
    })?;
    let q = *t * first;
    let dx = (target.x0 - q.x).max(0.0).max(q.x - target.x1);
    let dy = (target.y0 - q.y).max(0.0).max(q.y - target.y1);
    let reach = target.width().hypot(target.height()) + dx.hypot(dy);
    let (_, s_min) = singular_values(t);
    (s_min > 1e-9).then(|| reach / s_min)
}

/// The result of expanding a stroke in local space.
pub(crate) struct StrokeOutcome {
    /// Fold this factor into the premultiplied brush color (1.0 unless a hairline).
    pub alpha: f32,
    /// Whether the stroke produced geometry (`false` = zero/non-finite width).
    pub drew: bool,
    /// The dash guard's fallback, if it fired (the caller records it).
    pub dash_fallback: Option<Unsupported>,
    /// The width was clamped to `2 · max_useful_half_width` (the caller records it).
    pub width_clamped: bool,
}

/// Expand `path`'s stroke in **local** space into `ctx.output()`. Width, dashes, joins and
/// caps are interpreted in the path's own space (02 §6.3); the caller fill-tessellates the
/// outline under `transform` (`PathTessellator::fill_stroke_outline`) and folds `alpha` into
/// the color. `target` (device px, the target ∩ the current clip) bounds pathological widths
/// (11 §2.4).
pub(crate) fn expand(
    path: &BezPath,
    style: &Stroke,
    transform: Affine,
    target: kurbo::Rect,
    tol_scale: f64,
    ctx: &mut kurbo::StrokeCtx,
) -> StrokeOutcome {
    // Width clamp (S04): a half-width beyond `h_max` cannot change coverage inside `target`.
    let h_max = max_useful_half_width(path, &transform, target);
    let width_clamped = h_max.is_some_and(|h| style.width / 2.0 > h);
    let width = match h_max {
        Some(h) if width_clamped => 2.0 * h,
        _ => style.width,
    };
    // Device width = the geometric mean under non-uniform scale (02 §5.5).
    let wd = width * transform.determinant().abs().sqrt();
    if !width.is_finite() || width <= 0.0 {
        return StrokeOutcome { alpha: 1.0, drew: false, dash_fallback: None, width_clamped };
    }
    // Hairline rule: stroke at exactly 1 device px and fold the width into alpha.
    let (local_width, mut alpha) =
        if wd > 0.0 && wd < 1.0 { (width / wd, to_f32(wd).unwrap_or(0.0)) } else { (width, 1.0) };
    let s = transform.spectral_norm().max(1e-6);
    let mut style_adj = style.clone();
    style_adj.width = local_width;
    let mut dash_fallback = None;
    if let DashPlan::Solid { alpha: duty, kind } = plan_dashes(style, path, s) {
        style_adj.dash_pattern.clear();
        alpha *= duty;
        dash_fallback = Some(kind);
    }
    kurbo::stroke_with(
        path.elements().iter().copied(),
        &style_adj,
        &kurbo::StrokeOpts::default(),
        STROKE_TOLERANCE_PX * tol_scale / s,
        ctx,
    );
    StrokeOutcome { alpha, drew: true, dash_fallback, width_clamped }
}

#[cfg(test)]
#[path = "stroke_tests.rs"]
mod tests;
