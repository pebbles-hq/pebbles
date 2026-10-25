//! Shadow preparation (02 §8.2, R40): a rounded rect convolved with an isotropic Gaussian of
//! standard deviation σ (`std_dev`, the true Gaussian σ as in Flutter's `MaskFilter.blur`),
//! evaluated in the rect's LOCAL space so it is exact under any affine. Prepare only places the
//! quad and normalizes the parameters; `shadow.wgsl` evaluates the convolution per pixel.
//!
//! The math (written for this crate from the definition of the convolution): with the rect
//! centered at the origin, half extents `(hw, hh)` and corner radius `r`, each horizontal row
//! `y'` of the shape is the interval `|x'| ≤ X(y')`, where `X = hw` on the straight part
//! (`|y'| ≤ hh − r`) and `X = hw − r + √(r² − (|y'| − (hh − r))²)` across the corner arcs. The
//! Gaussian is separable, so the blurred value at `(x, y)` is
//!
//! ```text
//! ∫ g_σ(y − y') · [Φ((X(y') − x)/σ) − Φ((−X(y') − x)/σ)] dy',   Φ(z) = ½·(1 + erf(z/√2)).
//! ```
//!
//! The straight band has constant `X`, so its part is a product of two exact `Φ` differences.
//! For the two corner bands (height `r` each), write the row mass as
//! `Φ((X − x)/σ) + Φ((X + x)/σ) − 1`; in σ units around the corner (`u = (±x − (hw − r))/σ`,
//! `v` = the distance past `hh − r` in σ, `ρ = r/σ`) each band is then
//! `T(u₊, v; ρ) + T(u₋, v; ρ) − (Φ(v) − Φ(v − ρ))` with the precomputed blur profile
//! `T` (`execute/resources/shadow_profile.rs`, M33): a few texel loads per pixel instead of
//! ~40 erf evaluations, within 0.8 levels (of 255) of the converged integral.
//! Above the table's ρ the shader keeps the per-pixel integration: each band cut into slices
//! weighted by the exact Gaussian masses of their `y'` intervals, `X` at the slice midpoint.
//! See `shadow.wgsl` for the details and the erf approximation.

use std::ops::Range;

use kurbo::{Affine, Rect};
use peniko::Color;

use super::color::premul;
use super::num::{index_u32, narrow_f32};
use super::plan::FramePlan;
use crate::gpu_types::ShadowInstance;

/// The smallest σ: a non-positive `std_dev` draws the hard-edged rounded rect (R41).
const MIN_SIGMA: f32 = 1e-6;

/// The Gaussian's visible extent in σ: beyond 3σ less than 0.14 % of the mass remains.
const EXTENT_SIGMAS: f64 = 3.0;

/// erf(x) after Abramowitz & Stegun, *Handbook of Mathematical Functions* (1964), formula
/// 7.1.26 (absolute error ≤ 1.5·10⁻⁷ for x ≥ 0), extended to x < 0 by oddness. `shadow.wgsl`
/// evaluates the same formula; this copy documents and tests it.
#[cfg(test)]
pub(crate) fn erf(x: f32) -> f32 {
    const P: f32 = 0.327_591_1;
    const A: [f32; 5] = [0.254_829_6, -0.284_496_74, 1.421_413_7, -1.453_152_1, 1.061_405_4];
    let ax = x.abs();
    let t = 1.0 / (1.0 + P * ax);
    let poly = t * (A[0] + t * (A[1] + t * (A[2] + t * (A[3] + t * A[4]))));
    let y = 1.0 - poly * (-ax * ax).exp();
    y.copysign(x)
}

/// The normalized local-space shape: the rect centered at the origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ShadowParams {
    pub half_width: f32,
    pub half_height: f32,
    /// Corner radius, clamped to `[0, min(half_width, half_height)]`.
    pub radius: f32,
    /// Gaussian σ (local units), at least `MIN_SIGMA`.
    pub sigma: f32,
}

/// `None` for an empty or non-finite rect or non-finite parameters (R41: skip the op).
pub(crate) fn shadow_params(rect: Rect, radius: f32, std_dev: f32) -> Option<ShadowParams> {
    let r = rect.abs();
    let (w, h) = (narrow_f32(r.width()), narrow_f32(r.height()));
    if !(w.is_finite() && h.is_finite() && radius.is_finite() && std_dev.is_finite()) {
        return None;
    }
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let (half_width, half_height) = (0.5 * w, 0.5 * h);
    Some(ShadowParams {
        half_width,
        half_height,
        radius: radius.clamp(0.0, half_width.min(half_height)),
        sigma: std_dev.max(MIN_SIGMA),
    })
}

/// Pushes one shadow instance for `rect` under `xf`; `None` when nothing is drawn.
pub(crate) fn shadow(
    plan: &mut FramePlan,
    rect: &Rect,
    xf: Affine,
    radius: f64,
    std_dev: f64,
    color: Color,
    max_coord_px: f32,
) -> Option<Range<u32>> {
    let params = shadow_params(*rect, narrow_f32(radius), narrow_f32(std_dev))?;
    let r = rect.abs();
    let [a, b, c, d, _, _] = xf.as_coeffs();
    let origin = xf * r.origin();
    let extent = kernel_extent(f64::from(params.sigma), xf, max_coord_px);
    let instance = ShadowInstance {
        xform: [a, b, c, d].map(narrow_f32),
        origin_size: [
            narrow_f32(origin.x),
            narrow_f32(origin.y),
            narrow_f32(r.width()),
            narrow_f32(r.height()),
        ],
        params0: [params.half_width, params.half_height, params.radius, params.sigma],
        params1: [0.0; 4],
        color: premul(color),
        kernel: [extent, 0.0, 0.0, 0.0],
    };
    if !instance.xform.iter().chain(&instance.origin_size).all(|v| v.is_finite()) {
        return None;
    }
    let start = index_u32(plan.shadows.len());
    plan.shadows.push(instance);
    Some(start..start + 1)
}

/// How far (local units) the quad extends past the rect: `EXTENT_SIGMAS`·σ, limited so the
/// device-space inflation stays within `max_coord_px` (a huge σ must keep the quad finite).
fn kernel_extent(sigma: f64, m: Affine, max_coord_px: f32) -> f32 {
    let [a, b, c, d, _, _] = m.as_coeffs();
    // A local step of length 1 moves at most this far in device px (∞-norm bound).
    let stretch = (a.abs() + c.abs()).max(b.abs() + d.abs()).max(1e-12);
    narrow_f32((EXTENT_SIGMAS * sigma).min(f64::from(max_coord_px) / stretch))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execute::resources::shadow_profile as prof;

    /// `shadow.wgsl`'s per-pixel corner integration with `slices` slices per band (12: the pre-M33
    /// path, still the fallback above the table's ρ; 256: the reference).
    fn slices_value(x: f32, y: f32, [hw, hh, r, sig]: [f32; 4], slices: u16) -> f32 {
        let s = std::f32::consts::FRAC_1_SQRT_2 / sig;
        let rm = |x: f32, w: f32| 0.5 * (erf((x + w) * s) - erf((x - w) * s));
        let st = hh - r;
        let mut v = 0.5 * (erf((st - y) * s) - erf((-st - y) * s)) * rm(x, hw);
        if r > 0.0 {
            for (lo_in, hi_in) in [(st, hh), (-hh, -st)] {
                let (lo, hi) = (lo_in.max(y - 4.0 * sig), hi_in.min(y + 4.0 * sig));
                if hi <= lo {
                    continue;
                }
                let step = (hi - lo) / f32::from(slices);
                let mut ep = erf((lo - y) * s);
                for i in 0..slices {
                    let en = erf((lo + f32::from(i + 1) * step - y) * s);
                    let ia = (lo + (f32::from(i) + 0.5) * step).abs() - st;
                    let w = hw - r + (r * r - ia * ia).max(0.0).sqrt();
                    v += 0.5 * (en - ep) * rm(x, w);
                    ep = en;
                }
            }
        }
        v.clamp(0.0, 1.0)
    }

    /// `shadow.wgsl`'s profile lookup (bilinear in (u, v), linear in ρ).
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped ≥ 0 indices
    fn profile(u: f32, v: f32, rho: f32) -> f32 {
        let t = prof::texels();
        let level_at = |level: u32| {
            let n = prof::level_size(level);
            let (cx, cy) = (level % prof::COLS * prof::CELL, level / prof::COLS * prof::CELL);
            let fu = ((u + prof::PAD) / prof::STEP).clamp(0.0, (n - 1) as f32);
            let fv = ((v + prof::PAD) / prof::STEP).clamp(0.0, (n - 1) as f32);
            let (iu, iv) = ((fu as u32).min(n - 2), (fv as u32).min(n - 2));
            let (au, av) = (fu - iu as f32, fv - iv as f32);
            let at = |i: u32, j: u32| t[((cy + iv + j) * prof::WIDTH + cx + iu + i) as usize];
            let a = at(0, 0) + (at(1, 0) - at(0, 0)) * au;
            let b = at(0, 1) + (at(1, 1) - at(0, 1)) * au;
            a + (b - a) * av
        };
        let f = rho / prof::RHO_STEP;
        let level = (f as u32).min(prof::LEVELS - 2);
        let (a, b) = (level_at(level), level_at(level + 1));
        a + (b - a) * (f - level as f32)
    }

    /// `shadow.wgsl`'s value with the profile (ρ ≤ RHO_MAX).
    fn profile_value(x: f32, y: f32, [hw, hh, r, sig]: [f32; 4]) -> f32 {
        let (x, y) = (x.abs(), y.abs());
        let s = std::f32::consts::FRAC_1_SQRT_2 / sig;
        let st = hh - r;
        let wide = x + hw >= 4.0 * sig;
        let near = if wide { 1.0 } else { erf((x + hw) * s) };
        let mut v = 0.5 * (erf((st - y) * s) + erf((st + y) * s)) * 0.5 * (near - erf((x - hw) * s));
        let rho = r / sig;
        for vb in [(y - st) / sig, (-y - st) / sig] {
            if r == 0.0 || vb <= -prof::PAD || vb >= rho + prof::PAD {
                continue;
            }
            let core = hw - r;
            v += profile((x - core) / sig, vb, rho);
            let u_far = (-x - core) / sig;
            if u_far > -prof::PAD {
                let mass = 0.5
                    * (erf(vb * std::f32::consts::FRAC_1_SQRT_2)
                        - erf((vb - rho) * std::f32::consts::FRAC_1_SQRT_2));
                v += profile(u_far, vb, rho) - mass;
            }
        }
        v.clamp(0.0, 1.0)
    }

    /// M33: the profile path against the converged integral (256 slices) and against the
    /// 12-slice per-pixel integration it replaces, in levels of 255 at full opacity, over a sweep
    /// of σ, radius and size (ρ ≤ RHO_MAX; larger ρ still integrates per pixel).
    #[test]
    fn profile_matches_integration() {
        // (max, its shape, sum) against [reference, 12 slices].
        let mut acc = [(0f32, [0f32; 4], 0f64); 2];
        let mut n = 0u32;
        for sig in [0.5f32, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 14.0, 24.0, 40.0] {
            for r in [1.0f32, 3.0, 5.0, 8.0, 12.0, 16.0, 24.0, 32.0] {
                for (w, h) in [
                    (6.0f32, 6.0f32),
                    (20.0, 20.0),
                    (44.0, 60.0),
                    (60.0, 40.0),
                    (349.0, 150.0),
                    (30.0, 300.0),
                ] {
                    let (hw, hh) = (0.5 * w, 0.5 * h);
                    let r = r.min(hw).min(hh);
                    if r / sig > prof::RHO_MAX {
                        continue;
                    }
                    let shape = [hw, hh, r, sig];
                    let k = 3.0 * sig + 2.0;
                    for j in 0..=40u8 {
                        for i in 0..=40u8 {
                            let x = -hw - k + (w + 2.0 * k) * f32::from(i) / 40.0;
                            let y = -hh - k + (h + 2.0 * k) * f32::from(j) / 40.0;
                            let p = profile_value(x, y, shape);
                            for (a, slices) in acc.iter_mut().zip([256, 12]) {
                                let d = (p - slices_value(x, y, shape, slices)).abs() * 255.0;
                                if d > a.0 {
                                    (a.0, a.1) = (d, shape);
                                }
                                a.2 += f64::from(d);
                            }
                            n += 1;
                        }
                    }
                }
            }
        }
        let [(ref_max, ref_at, ref_sum), (old_max, old_at, old_sum)] = acc;
        let (ref_mean, old_mean) = (ref_sum / f64::from(n), old_sum / f64::from(n));
        assert!(ref_max <= 1.0 && ref_mean < 0.1, "vs converged: {ref_max} at {ref_at:?}, mean {ref_mean}");
        // The 12-slice path itself is up to ~2 levels off the converged integral at ρ ≈ 4.
        assert!(old_max <= 2.1 && old_mean < 0.1, "vs 12 slices: {old_max} at {old_at:?}, mean {old_mean}");
    }

    #[test]
    fn erf_values() {
        assert!(erf(0.0).abs() < 1e-6);
        assert!((erf(1.0) - 0.842_700_8).abs() < 1e-6, "{}", erf(1.0));
        assert!((erf(0.5) - 0.520_499_9).abs() < 1e-6, "{}", erf(0.5));
        assert!((erf(20.0) - 1.0).abs() < 1e-6);
        for x in [0.1f32, 0.7, 2.3] {
            assert_eq!(erf(-x), -erf(x));
        }
    }

    #[test]
    fn params_square_sigma10() {
        let p = shadow_params(Rect::new(0.0, 0.0, 100.0, 100.0), 0.0, 10.0).unwrap();
        assert_eq!(p, ShadowParams { half_width: 50.0, half_height: 50.0, radius: 0.0, sigma: 10.0 });
    }

    #[test]
    fn params_clamp_radius() {
        let clamped = shadow_params(Rect::new(0.0, 0.0, 100.0, 50.0), 80.0, 6.0).unwrap();
        let exact = shadow_params(Rect::new(0.0, 0.0, 100.0, 50.0), 25.0, 6.0).unwrap();
        assert_eq!(clamped, exact);
    }

    #[test]
    fn non_finite_params_skip() {
        let r = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert_eq!(shadow_params(r, f32::NAN, 2.0), None);
        assert_eq!(shadow_params(r, 2.0, f32::INFINITY), None);
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

    /// M33: a shadow followed by an opaque fill gets the fill's interior as its hole (centered
    /// local units, ≥ 0.3·r + 2 px inside, on the 1/16 grid); anything else gets none.
    #[test]
    fn opaque_fill_occludes_shadow_interior() {
        use pebbles_render::paint::{Painter, Scene};
        let hole = |fill: Color, radius: f64, clip: bool| {
            let mut s = Scene::new();
            let mut p = Painter::new(&mut s);
            let r = Rect::new(20.0, 20.0, 80.0, 60.0);
            if clip {
                p.push_layer(
                    peniko::Fill::NonZero,
                    peniko::Mix::Normal,
                    1.0,
                    Affine::IDENTITY,
                    &kurbo::Circle::new((50.0, 40.0), 30.0),
                );
            }
            p.draw_blurred_rounded_rect(Affine::IDENTITY, r, Color::BLACK, radius, 6.0);
            p.fill(peniko::Fill::NonZero, Affine::IDENTITY, fill, None, &r.to_rounded_rect(radius));
            if clip {
                p.pop_layer();
            }
            crate::prepare::test_util::prepare_scene(&s, 100, 100).0.shadows[0].params1
        };
        // r = 8: m = ceil((0.3·8 + 2)·16)/16 = 4.4375; the rect is 60 × 40 (half 30 × 20).
        assert_eq!(hole(Color::WHITE, 8.0, false), [-25.5625, -15.5625, 25.5625, 15.5625]);
        assert_eq!(hole(Color::WHITE, 0.0, false), [-28.0, -18.0, 28.0, 18.0]);
        assert_eq!(hole(Color::from_rgba8(255, 255, 255, 254), 8.0, false), [0.0; 4], "translucent");
        assert_eq!(hole(Color::WHITE, 8.0, true), [0.0; 4], "under an SDF clip");
    }
}
