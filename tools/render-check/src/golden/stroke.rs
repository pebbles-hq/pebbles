//! Stroke coverage (05 §6.7a): `kurbo::stroke` in local space at `(1/1024) / scale`, then
//! [`fill_coverage`]; the 02 §5.5 hairline rule.

use kurbo::{Affine, BezPath, Stroke, StrokeOpts};
use peniko::Fill;

use super::coverage::{FLATTEN_TOL_PX, fill_coverage};

/// The device width of a local `width` under `t` (geometric mean under non-uniform scale).
pub fn device_width(width: f64, t: Affine) -> f64 {
    width * t.determinant().abs().sqrt()
}

/// The stroke outline of `path` in local space.
pub fn outline(path: &BezPath, style: &Stroke, t: Affine) -> BezPath {
    let scale = t.spectral_norm().max(1e-12);
    kurbo::stroke(path.elements().iter().copied(), style, &StrokeOpts::default(), FLATTEN_TOL_PX / scale)
}

/// Coverage of `path` stroked with `style` under `t`. A device width in `(0, 1)` strokes at
/// exactly 1 device px with the coverage scaled by the device width (02 §5.5); a zero or
/// non-finite width covers nothing.
pub fn stroke_coverage(path: &BezPath, style: &Stroke, t: Affine, w: u32, h: u32) -> Vec<f64> {
    let wd = device_width(style.width, t);
    if !(style.width.is_finite() && style.width > 0.0 && wd > 0.0) {
        return vec![0.0; w as usize * h as usize];
    }
    if wd >= 1.0 {
        return fill_coverage(&outline(path, style, t), t, Fill::NonZero, w, h);
    }
    let mut hair = style.clone();
    hair.width = style.width / wd;
    let mut cov = fill_coverage(&outline(path, &hair, t), t, Fill::NonZero, w, h);
    cov.iter_mut().for_each(|c| *c *= wd);
    cov
}
