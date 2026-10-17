//! Tier A analytic shapes (02 §6.4, R10): eligibility and device geometry for rect / rounded-rect
//! / circle fills drawn as one SDF-covered instanced quad.

use kurbo::{Affine, Rect};
use pebbles_render::paint::display_list::Geometry;

use super::num::to_f32;
use crate::gpu_types::ShapeInstance;

/// Scale only, positive, no rotation/skew/mirror: `|b|,|c| ≤ 1e-9·max(|a|,|d|)`, `a > 0`, `d > 0`.
fn axis_positive(m: Affine) -> bool {
    let [a, b, c, d, ..] = m.as_coeffs();
    let s = a.abs().max(d.abs());
    b.abs() <= 1e-9 * s && c.abs() <= 1e-9 * s && a > 0.0 && d > 0.0
}

/// `|a − d| ≤ 1e-6·a` (required once any radius > 0, or for circles).
fn uniform(m: Affine) -> bool {
    let [a, _, _, d, ..] = m.as_coeffs();
    (a - d).abs() <= 1e-6 * a
}

fn device_rect(r: Rect, m: Affine) -> Rect {
    let [a, _, _, d, e, f] = m.as_coeffs();
    Rect::new(a * r.x0 + e, d * r.y0 + f, a * r.x1 + e, d * r.y1 + f)
}

fn f4(v: [f64; 4]) -> Option<[f32; 4]> {
    Some([to_f32(v[0])?, to_f32(v[1])?, to_f32(v[2])?, to_f32(v[3])?])
}

/// The device rect and radii (tl, tr, br, bl) of an eligible shape, or `None` (tier selection).
pub(crate) fn device_shape(g: &Geometry, m: Affine) -> Option<(Rect, [f64; 4])> {
    if !axis_positive(m) || !m.as_coeffs().iter().all(|v| v.is_finite()) {
        return None;
    }
    let a = m.as_coeffs()[0];
    let (rect, radii) = match g {
        Geometry::Rect(r) => (device_rect(r.abs(), m), [0.0; 4]),
        Geometry::RoundedRect(rr) => {
            let r = rr.radii();
            let radii = [r.top_left, r.top_right, r.bottom_right, r.bottom_left];
            if radii.iter().any(|v| *v > 0.0) && !uniform(m) {
                return None;
            }
            (device_rect(rr.rect().abs(), m), radii.map(|v| a * v))
        }
        Geometry::Circle(c) => {
            if !uniform(m) {
                return None;
            }
            let r = Rect::from_center_size(c.center, (2.0 * c.radius, 2.0 * c.radius));
            (device_rect(r, m), [a * c.radius; 4])
        }
        _ => return None,
    };
    let finite = [rect.x0, rect.y0, rect.x1, rect.y1].iter().chain(&radii).all(|v| v.is_finite());
    (finite && rect.width() > 0.0 && rect.height() > 0.0).then_some((rect, radii))
}

/// A Tier A fill instance for `g` under `m` with premultiplied `color`, or `None` if ineligible.
pub(crate) fn fill_instance(g: &Geometry, m: Affine, color: [f32; 4]) -> Option<ShapeInstance> {
    let (rect, radii) = device_shape(g, m)?;
    Some(ShapeInstance {
        outer: f4([rect.x0, rect.y0, rect.x1, rect.y1])?,
        outer_radii: f4(radii)?,
        inner: [0.0; 4],
        inner_radii: [0.0; 4],
        color,
    })
}

/// Clamp a sharp-cornered instance's outer rect to `scissor` inflated by 2 px.
pub(crate) fn clamp_outer(inst: &mut ShapeInstance, scissor: Rect) {
    let s = scissor.inflate(2.0, 2.0);
    let [x0, y0, x1, y1] = inst.outer.map(f64::from);
    let c = Rect::new(x0.max(s.x0), y0.max(s.y0), x1.min(s.x1), y1.min(s.y1));
    if let Some(outer) = f4([c.x0, c.y0, c.x1.max(c.x0), c.y1.max(c.y0)]) {
        inst.outer = outer;
    }
}

#[cfg(test)]
#[path = "shapes_tests.rs"]
mod tests;
