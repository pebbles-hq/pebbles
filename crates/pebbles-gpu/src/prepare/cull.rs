//! Conservative device bounds, culling against the active scissor, and the huge-rect clamp
//! (02 §6.1, ORD-7, OC-57). Culling only drops ops that cannot touch a pixel of the scissor,
//! so it never changes output.

use kurbo::{Affine, Join, Rect, Stroke};
use pebbles_render::paint::display_list::Geometry;

use super::plan::ScissorRect;

/// AA margin added to every device bound (px).
const AA_MARGIN_PX: f64 = 1.0;
/// Margin kept around the scissor when clamping huge rects (px).
const CLAMP_MARGIN_PX: f64 = 2.0;

/// Conservative device-space bounds of `g` under `m`, including the stroke's half-width
/// (miter joins at their limit) and a 1 px AA margin.
pub(crate) fn device_bounds(g: &Geometry, m: Affine, stroke: Option<&Stroke>) -> Rect {
    let b = m.transform_rect_bbox(g.bounding_box());
    let pad = match stroke {
        Some(style) => {
            let k = if style.join == Join::Miter { style.miter_limit.max(1.0) } else { 1.0 };
            0.5 * style.width * m.spectral_norm() * k + AA_MARGIN_PX
        }
        None => AA_MARGIN_PX,
    };
    b.inflate(pad, pad)
}

/// Device bounds of an image of `w`×`h` px placed by `m`.
pub(crate) fn image_bounds(w: u32, h: u32, m: Affine) -> Rect {
    m.transform_rect_bbox(Rect::new(0.0, 0.0, f64::from(w), f64::from(h)))
}

/// Device bounds of a blurred rect: inflated by `2.5 · std_dev` in local space, then transformed.
pub(crate) fn blurred_bounds(rect: Rect, std_dev: f64, m: Affine) -> Rect {
    let r = 2.5 * std_dev.abs();
    m.transform_rect_bbox(rect.inflate(r, r))
}

/// The scissor as an f64 rect.
fn scissor_rect(s: ScissorRect) -> Rect {
    Rect::new(f64::from(s.x0), f64::from(s.y0), f64::from(s.x1), f64::from(s.y1))
}

/// Whether `b` overlaps the scissor (open intervals: touching an edge is no coverage).
/// NaN bounds never cull (R02 skips them first).
pub(crate) fn is_outside(b: Rect, s: ScissorRect) -> bool {
    let s = scissor_rect(s);
    b.x1 <= s.x0 || b.x0 >= s.x1 || b.y1 <= s.y0 || b.y0 >= s.y1
}

/// For an axis-aligned `Geometry::Rect` fill, the device rect intersected with the scissor
/// inflated by 2 px (keeps f32 vertices small for `Rect::new(-1e9, -1e9, 1e9, 1e9)`).
/// `None` when the geometry is not a rect or the transform rotates/skews.
pub(crate) fn clamped_rect(g: &Geometry, m: Affine, s: ScissorRect) -> Option<Rect> {
    let Geometry::Rect(r) = g else { return None };
    let [a, b, c, d, _, _] = m.as_coeffs();
    let axis_aligned = (b == 0.0 && c == 0.0) || (a == 0.0 && d == 0.0);
    if !axis_aligned {
        return None;
    }
    let limit = scissor_rect(s).inflate(CLAMP_MARGIN_PX, CLAMP_MARGIN_PX);
    Some(m.transform_rect_bbox(*r).intersect(limit))
}

#[cfg(test)]
#[path = "cull_tests.rs"]
mod tests;
