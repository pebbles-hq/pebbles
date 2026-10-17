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

#[cfg(test)]
#[path = "hygiene_tests.rs"]
mod tests;
