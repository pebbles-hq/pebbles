//! Deterministic tier selection per op (02 §5.2, R16): a pure function of the op and its
//! device transform — never of timing, frame counters or cache history.

use kurbo::{Affine, PathEl};
use pebbles_render::paint::display_list::{Geometry, Op, PathHint};

use super::shapes;

/// Largest device-px area of a Tier B mask (512²).
pub(crate) const MASK_MAX_AREA: u64 = 262_144;
/// Most flattened segments of a Tier B path.
pub(crate) const MASK_MAX_SEGMENTS: u32 = 20_000;
/// Flattening tolerance used to count segments (device px).
const COUNT_TOL_PX: f64 = 0.25;

/// The rasterization tier of one op (02 §5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tier {
    /// Analytic primitive (SDF shapes, shadows).
    A,
    /// Cached CPU coverage mask (glyphs, small paths).
    B,
    /// Tessellated mesh (large or volatile paths).
    C,
}

/// The device-px area of the mask `g` would need: its bounds (grown by the stroke's half width
/// and miter reach) rounded out to whole pixels.
pub(crate) fn mask_area(g: &Geometry, device: Affine, stroke: Option<&kurbo::Stroke>) -> u64 {
    let mut b = device.transform_rect_bbox(g.bounding_box());
    if let Some(s) = stroke {
        let k = if s.join == kurbo::Join::Miter { s.miter_limit.max(1.0) } else { 1.0 };
        let pad = 0.5 * s.width * device.spectral_norm() * k;
        b = b.inflate(pad, pad);
    }
    let side = |lo: f64, hi: f64| (hi.ceil() - lo.floor()).max(0.0);
    let area = side(b.x0, b.x1) * side(b.y0, b.y1);
    if area.is_nan() { u64::MAX } else { super::num::to_u64_sat(area) }
}

/// Flattened segment count of `g` under `device`, saturating at `MASK_MAX_SEGMENTS + 1`.
pub(crate) fn segment_count(g: &Geometry, device: Affine) -> u32 {
    let s = device.spectral_norm().max(1e-9);
    let path = g.to_path(COUNT_TOL_PX / s);
    let mut n = 0u32;
    kurbo::flatten(path.elements().iter().copied(), COUNT_TOL_PX / s, |el| {
        if matches!(el, PathEl::LineTo(_) | PathEl::ClosePath) {
            n = n.saturating_add(1).min(MASK_MAX_SEGMENTS + 1);
        }
    });
    n
}

/// The tier of `op` drawn under `device` (02 §5.2).
pub(crate) fn choose_tier(op: &Op, device: Affine) -> Tier {
    let (shape, stroke, hint) = match op {
        Op::Fill { shape, hint, .. } => {
            if shapes::fill_instance(shape, device, [0.0; 4]).is_some() {
                return Tier::A;
            }
            (shape, None, *hint)
        }
        Op::Stroke { shape, style, hint, .. } => {
            if shapes::stroke_instance(shape, device, style, [0.0; 4]).is_some() {
                return Tier::A;
            }
            (shape, Some(style), *hint)
        }
        Op::Glyphs { .. } => return Tier::B,
        _ => return Tier::A,
    };
    if hint == PathHint::Auto
        && mask_area(shape, device, stroke) <= MASK_MAX_AREA
        && segment_count(shape, device) <= MASK_MAX_SEGMENTS
    {
        Tier::B
    } else {
        Tier::C
    }
}

#[cfg(test)]
#[path = "path_strategy_tests.rs"]
mod tests;
