//! Input hygiene (02 §6.1, 11 §2.3): finiteness and emptiness checks that run before any
//! prepare work, so lyon and the arenas never see NaN or infinity.

use kurbo::PathEl;
use pebbles_render::paint::display_list::Geometry;

/// Every coordinate of `g` is finite.
pub(crate) fn geometry_is_finite(g: &Geometry) -> bool {
    let fin = |v: &[f64]| v.iter().all(|x| x.is_finite());
    match g {
        Geometry::Rect(r) => fin(&[r.x0, r.y0, r.x1, r.y1]),
        Geometry::RoundedRect(rr) => {
            let (r, k) = (rr.rect(), rr.radii());
            fin(&[r.x0, r.y0, r.x1, r.y1, k.top_left, k.top_right, k.bottom_right, k.bottom_left])
        }
        Geometry::Circle(c) => fin(&[c.center.x, c.center.y, c.radius]),
        Geometry::Path(path) => path.elements().iter().all(|el| match *el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => fin(&[p.x, p.y]),
            PathEl::QuadTo(a, p) => fin(&[a.x, a.y, p.x, p.y]),
            PathEl::CurveTo(a, b, p) => fin(&[a.x, a.y, b.x, b.y, p.x, p.y]),
            PathEl::ClosePath => true,
        }),
        // `Geometry` is `#[non_exhaustive]`: a future variant is judged by its bounds.
        _ => g.bounding_box().is_finite(),
    }
}

/// `g` covers no area: a zero-width or zero-height (rounded) rect, a circle with radius
/// `<= 0`, or a path with no drawing element after its first `MoveTo`.
pub(crate) fn geometry_is_empty(g: &Geometry) -> bool {
    match g {
        Geometry::Rect(r) => {
            let r = r.abs();
            r.width() == 0.0 || r.height() == 0.0
        }
        Geometry::RoundedRect(rr) => {
            let r = rr.rect().abs();
            r.width() == 0.0 || r.height() == 0.0
        }
        Geometry::Circle(c) => c.radius <= 0.0,
        Geometry::Path(path) => !path
            .elements()
            .iter()
            .skip_while(|el| !matches!(el, PathEl::MoveTo(_)))
            .any(|el| matches!(el, PathEl::LineTo(_) | PathEl::QuadTo(..) | PathEl::CurveTo(..))),
        _ => false,
    }
}

/// Every component of a solid colour is finite.
pub(crate) fn color_is_finite(c: &peniko::Color) -> bool {
    c.components.iter().all(|v| v.is_finite())
}

/// Every component of every gradient stop colour is finite.
pub(crate) fn stop_colors_are_finite(g: &peniko::Gradient) -> bool {
    g.stops.iter().all(|s| s.color.components.iter().all(|v| v.is_finite()))
}

/// The op transform collapses area (a legitimate animation state: skipped silently).
pub(crate) fn transform_is_singular(t: &kurbo::Affine) -> bool {
    t.determinant().abs() < 1e-12
}

/// (sanitized alpha, was_non_finite): NaN/inf → (0.0, true); otherwise clamp to 0..=1.
pub(crate) fn sanitize_layer_alpha(alpha: f32) -> (f32, bool) {
    if alpha.is_finite() { (alpha.clamp(0.0, 1.0), false) } else { (0.0, true) }
}

/// Every corner of `device_bbox` lies within `±limits.max_coord_px`.
pub(crate) fn coords_in_range(device_bbox: kurbo::Rect, limits: &crate::RendererLimits) -> bool {
    let m = f64::from(limits.max_coord_px);
    [device_bbox.x0, device_bbox.y0, device_bbox.x1, device_bbox.y1].iter().all(|v| v.abs() <= m)
}

#[cfg(test)]
#[path = "hygiene_tests.rs"]
mod tests;
