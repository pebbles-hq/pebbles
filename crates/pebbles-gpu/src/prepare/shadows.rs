//! Shadow preparation (02 §8.2, R40): the closed-form Gaussian blur of a rounded rect,
//! evaluated in the rect's local space so it is exact under any affine. The parameters are
//! computed once on the CPU; `shadow.wgsl` evaluates them per pixel. Constants credited to
//! vello_common-0.2.0 `src/encode.rs:899-970` and `erf7` to `src/math.rs:20-29` (Apache-2.0 OR
//! MIT; formulas, re-typed from the spec).

use std::ops::Range;

use kurbo::{Affine, Rect};
use peniko::Color;

use super::color::premul;
use super::num::{index_u32, narrow_f32};
use super::plan::FramePlan;
use crate::gpu_types::ShadowInstance;

/// A 7th-order rational approximation of `erf`.
pub(crate) fn erf7(x: f32) -> f32 {
    let x = x.clamp(-10.0, 10.0) * std::f32::consts::FRAC_2_SQRT_PI;
    let xx = x * x;
    let x = x + (0.24295 + (0.03395 + 0.0104 * xx) * xx) * (x * xx);
    x / (1.0 + x * x).sqrt()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShadowParams {
    pub width: f32,
    pub height: f32,
    pub exponent: f32,
    pub recip_exponent: f32,
    pub scale: f32,
    pub std_dev_inv: f32,
    pub min_edge: f32,
    pub w: f32,
    pub h: f32,
    pub r1: f32,
}

/// `None` when the rect is degenerate (min edge ≤ 0) or any input is non-finite (R41).
pub(crate) fn shadow_params(rect: Rect, radius: f32, std_dev: f32) -> Option<ShadowParams> {
    let rect = rect.abs();
    let width = narrow_f32(rect.width());
    let height = narrow_f32(rect.height());
    if !(width.is_finite() && height.is_finite() && radius.is_finite() && std_dev.is_finite()) {
        return None;
    }
    let min_edge = width.min(height);
    if min_edge <= 0.0 {
        return None;
    }
    let radius = radius.max(0.0).min(0.5 * min_edge);
    let std_dev = std_dev.max(1e-6);
    let rmax = 0.5 * min_edge;
    let r0 = radius.hypot(std_dev * 1.15).min(rmax);
    let r1 = radius.hypot(std_dev * 2.0).min(rmax);
    let exponent = 2.0 * r1 / r0;
    let std_dev_inv = std_dev.recip();
    let delta = 1.25
        * std_dev
        * ((-(0.5 * std_dev_inv * width).powi(2)).exp() - (-(0.5 * std_dev_inv * height).powi(2)).exp());
    let w = width + delta.min(0.0);
    let h = height - delta.max(0.0);
    let scale = 0.5 * erf7(std_dev_inv * 0.5 * (w.max(h) - 0.5 * radius));
    Some(ShadowParams {
        width,
        height,
        exponent,
        recip_exponent: exponent.recip(),
        scale,
        std_dev_inv,
        min_edge,
        w,
        h,
        r1,
    })
}

/// Append one shadow instance for `rect` under `m` (local → device) and return its range;
/// `None` when the rect is empty (R41: skipped silently). `max_coord_px` bounds the quad
/// inflation (S05): beyond it the shadow is outside every possible target.
pub(crate) fn shadow(
    plan: &mut FramePlan,
    rect: &Rect,
    m: Affine,
    radius: f64,
    std_dev: f64,
    color: Color,
    max_coord_px: f32,
) -> Option<Range<u32>> {
    let p = shadow_params(*rect, narrow_f32(radius), narrow_f32(std_dev))?;
    let r = rect.abs();
    let [a, b, c, d, _, _] = m.as_coeffs().map(narrow_f32);
    let o = m * kurbo::Point::new(r.x0, r.y0);
    let start = index_u32(plan.shadows.len());
    plan.shadows.push(ShadowInstance {
        xform: [a, b, c, d],
        origin_size: [narrow_f32(o.x), narrow_f32(o.y), p.width, p.height],
        // (corner radius clamped to half the min edge, Gaussian σ = std_dev (Skia/Flutter), 0, 0).
        params0: [narrow_f32(radius).max(0.0).min(0.5 * p.min_edge), narrow_f32(std_dev).max(1e-6), 0.0, 0.0],
        params1: [0.0; 4],
        color: premul(color),
        kernel: [kernel_extent(std_dev, m, max_coord_px), 0.0, 0.0, 0.0],
    });
    Some(start..start + 1)
}

/// `3 · std_dev` (local units), clamped to `max_coord_px / σmax(M)` so the inflated quad stays
/// finite for absurd σ (S05). σ itself is never clamped (R41).
fn kernel_extent(std_dev: f64, m: Affine, max_coord_px: f32) -> f32 {
    // 3σ: the Gaussian's visible extent (Skia's blur kernel extent).
    let k = 3.0 * std_dev.max(1e-6);
    let (smax, _) = super::stroke::singular_values(&m);
    let bound = if smax > 0.0 { f64::from(max_coord_px) / smax } else { k };
    narrow_f32(k.min(bound))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erf7_values() {
        assert_eq!(erf7(0.0), 0.0);
        assert!((erf7(1.0) - 0.8427).abs() < 1e-3, "{}", erf7(1.0));
        assert!((erf7(20.0) - 1.0).abs() < 1e-6);
        for x in [0.1f32, 0.7, 2.3] {
            assert_eq!(erf7(-x), -erf7(x));
        }
    }

    #[test]
    fn params_square_sigma10() {
        let p = shadow_params(Rect::new(0.0, 0.0, 100.0, 100.0), 0.0, 10.0).unwrap();
        assert!((p.exponent - 3.4783).abs() < 1e-3, "{}", p.exponent);
        assert_eq!((p.w, p.h), (100.0, 100.0));
        assert!((p.scale - 0.5).abs() < 1e-3, "{}", p.scale);
    }

    #[test]
    fn params_clamp_radius() {
        let clamped = shadow_params(Rect::new(0.0, 0.0, 100.0, 50.0), 80.0, 6.0).unwrap();
        let exact = shadow_params(Rect::new(0.0, 0.0, 100.0, 50.0), 25.0, 6.0).unwrap();
        assert_eq!(clamped, exact);
    }

    fn one(rect: Rect, radius: f64, std_dev: f64, color: Color) -> (FramePlan, crate::stats::FrameStats) {
        use pebbles_render::paint::{Painter, Scene};
        let mut s = Scene::new();
        Painter::new(&mut s).draw_blurred_rounded_rect(Affine::IDENTITY, rect, color, radius, std_dev);
        crate::prepare::test_util::prepare_scene(&s, 200, 200)
    }

    #[test]
    fn nan_rect_shadow_skipped() {
        let (plan, stats) = one(Rect::new(f64::NAN, 0.0, 10.0, 10.0), 0.0, 4.0, Color::BLACK);
        assert!(plan.shadows.is_empty());
        assert_eq!(stats.non_finite_skipped, 1);
    }

    #[test]
    fn zero_size_shadow_skipped() {
        let (plan, stats) = one(Rect::new(10.0, 10.0, 10.0, 50.0), 0.0, 4.0, Color::BLACK);
        assert!(plan.shadows.is_empty() && plan.cmds.is_empty());
        assert_eq!(stats.non_finite_skipped, 0, "silent");
    }

    #[test]
    fn transparent_shadow_skipped() {
        let (plan, _) = one(Rect::new(10.0, 10.0, 50.0, 50.0), 0.0, 4.0, Color::TRANSPARENT);
        assert!(plan.shadows.is_empty() && plan.cmds.is_empty());
    }

    #[test]
    fn negative_radius_is_zero() {
        let r = Rect::new(0.0, 0.0, 100.0, 60.0);
        assert_eq!(shadow_params(r, -5.0, 6.0), shadow_params(r, 0.0, 6.0));
        assert_eq!(shadow_params(r, 4.0, -3.0), shadow_params(r, 4.0, 1e-6), "negative σ → hard edge");
    }

    #[test]
    fn inverted_rect_is_normalized() {
        let a = one(Rect::new(150.0, 150.0, 50.0, 50.0), 4.0, 6.0, Color::BLACK).0.shadows;
        let b = one(Rect::new(50.0, 50.0, 150.0, 150.0), 4.0, 6.0, Color::BLACK).0.shadows;
        assert_eq!(a, b);
    }

    #[test]
    fn huge_sigma_keeps_quad_finite() {
        let limits = crate::test_support::webgl2_limits();
        let mut plan = FramePlan::default();
        let r = Rect::new(0.0, 0.0, 100.0, 100.0);
        shadow(&mut plan, &r, Affine::IDENTITY, 0.0, 1e30, Color::BLACK, limits.max_coord_px).unwrap();
        let s = plan.shadows[0];
        let k = s.kernel[0];
        assert!(k.is_finite());
        // Quad corners: (−k, −k) … (w + k, h + k) in local = device units here.
        let far = f64::from(s.origin_size[2] + k);
        assert!(far <= f64::from(limits.max_coord_px) + 100.0, "{far}");
        assert!(s.params0.iter().chain(&s.params1).all(|v| v.is_finite()));
    }

    #[test]
    fn normal_sigma_is_unchanged() {
        let limits = crate::test_support::webgl2_limits();
        let mut plan = FramePlan::default();
        let r = Rect::new(0.0, 0.0, 100.0, 100.0);
        shadow(&mut plan, &r, Affine::IDENTITY, 0.0, 8.0, Color::BLACK, limits.max_coord_px).unwrap();
        assert_eq!(plan.shadows[0].kernel[0], 3.0 * 8.0);
    }

    #[test]
    fn shadow_stays_before_card_fill() {
        use pebbles_render::paint::{Painter, Scene};
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        let r = Rect::new(20.0, 20.0, 80.0, 80.0);
        p.draw_blurred_rounded_rect(Affine::IDENTITY, r, Color::BLACK, 8.0, 6.0);
        p.fill(peniko::Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &r.to_rounded_rect(8.0));
        let (plan, _) = crate::prepare::test_util::prepare_scene(&s, 100, 100);
        let kinds = crate::prepare::test_util::cmd_kinds(&plan);
        assert!(kinds == ["shadows", "triangles"] || kinds == ["shadows", "shapes"], "{kinds:?}");
    }
}
