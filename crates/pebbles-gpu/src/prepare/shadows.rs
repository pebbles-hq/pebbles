//! Shadow preparation: one `ShadowInstance` per blurred rounded rect.

use std::ops::Range;

use kurbo::{Affine, Rect};
use peniko::Color;

use super::color::premul;
use super::num::{index_u32, to_f32};
use super::plan::FramePlan;
use crate::gpu_types::ShadowInstance;

/// Append one shadow instance (an inflated analytic quad) and return its arena range.
pub(crate) fn shadow(
    plan: &mut FramePlan,
    rect: &Rect,
    transform: Affine,
    radius: f64,
    std_dev: f64,
    color: Color,
) -> Range<u32> {
    let coeffs = transform.as_coeffs();
    let scale = (coeffs[0] * coeffs[0] + coeffs[1] * coeffs[1]).sqrt().max(1e-6);
    let c00 = transform * kurbo::Point::new(rect.x0, rect.y0);
    let c11 = transform * kurbo::Point::new(rect.x1, rect.y1);
    let c10 = transform * kurbo::Point::new(rect.x1, rect.y0);
    let c01 = transform * kurbo::Point::new(rect.x0, rect.y1);
    let bounds = [
        to_f32(c00.x.min(c10.x).min(c11.x).min(c01.x)).unwrap_or(0.0),
        to_f32(c00.y.min(c10.y).min(c11.y).min(c01.y)).unwrap_or(0.0),
        to_f32(c00.x.max(c10.x).max(c11.x).max(c01.x)).unwrap_or(0.0),
        to_f32(c00.y.max(c10.y).max(c11.y).max(c01.y)).unwrap_or(0.0),
    ];
    let radius = to_f32(radius * scale).unwrap_or(0.0);
    let sigma = to_f32(std_dev * scale).unwrap_or(0.0);
    let color = premul(color);
    let start = index_u32(plan.shadows.len());
    plan.shadows.push(ShadowInstance { rect: bounds, color, params: [radius, sigma, 0.0, 0.0] });
    start..start + 1
}
