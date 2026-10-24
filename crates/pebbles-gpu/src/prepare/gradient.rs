//! Gradients (09 §5): validation (§5.1), ramp encoding and baking (§5.2) and the
//! gradient-space transform of every kind (§5.4).
//!
//! The geometry is written from the public definitions: CSS Images Level 3/4 for linear and
//! conic (sweep) gradients and the HTML Canvas 2D `createRadialGradient` definition for
//! two-circle radial gradients. Every kind maps a device pixel `p` to gradient space with one
//! affine `G`; the shader turns `G·p` into the ramp parameter `t`:
//!
//! - linear: `G` sends the start point to (0, 0) and the end point to (1, 0), so `t = x`;
//! - sweep: `G` moves the center to the origin; `t` is the angle in [0, 2π) rescaled so the
//!   start angle maps to 0 and the end angle to 1;
//! - radial with (nearly) one center: `G` moves the center to the origin and divides by
//!   `|r1 − r0|`, so `t = (|q|·|r1 − r0| − r0) / (r1 − r0)`, an affine function of `|q|`;
//! - radial with two centers (the Canvas 2D two-circle model): `G` sends `c0` to (0, 0) and
//!   `c1` to (1, 0), with the radii divided by the same length `d = |c1 − c0|`. The circle of
//!   parameter `ω` is centered at `(ω, 0)` with radius `ρ0 + ω·Δ` (`ρ0 = r0/d`,
//!   `Δ = (r1 − r0)/d`), so a point `q` lies on it when
//!   `(1 − Δ²)·ω² − 2·(q.x + ρ0·Δ)·ω + (|q|² − ρ0²) = 0`. The shader solves this quadratic
//!   and keeps the largest root whose radius is non-negative; no root means the pixel is
//!   outside every circle and paints nothing.
use kurbo::{Affine, Point};

use super::num::narrow_f32;
use peniko::color::{AlphaColor, ColorSpaceTag, HueDirection, Srgb};
use peniko::{ColorStop, Gradient, GradientKind, InterpolationAlphaSpace};

/// Uniform sub-stops per stop pair for non-sRGB interpolation spaces (§5.2 step 3).
const SUBSTOPS: u32 = 64;

/// Largest ramp, in texels (§5.2).
pub(crate) const MAX_LUT_LEN: u32 = 2048;

const EPS: f64 = 1e-6;
const TOL: f64 = 1.0 / 4096.0;

/// 09 §5.1: `Err(color)` = draw the geometry as a solid of `color`.
pub(crate) fn validate_gradient(g: &Gradient, paint_xf: Affine) -> Result<(), peniko::Color> {
    let stops = &g.stops[..];
    let Some(first) = stops.first() else { return Err(peniko::color::palette::css::BLACK) };
    let first = first.color.to_alpha_color::<Srgb>();
    if stops.len() == 1 {
        return Err(first);
    }
    let in_range = stops.iter().all(|s| (0.0..=1.0).contains(&s.offset));
    let ascending = stops.iter().zip(stops.iter().skip(1)).all(|(a, b)| a.offset <= b.offset);
    if !in_range || !ascending {
        return Err(first);
    }
    let finite = |p: Point| p.x.is_finite() && p.y.is_finite();
    match g.kind {
        GradientKind::Linear(l) => {
            if !finite(l.start) || !finite(l.end) {
                return Err(first);
            }
            if (l.start.x - l.end.x).abs() <= EPS && (l.start.y - l.end.y).abs() <= EPS {
                return Err(first);
            }
        }
        GradientKind::Radial(r) => {
            let (r0, r1) = (f64::from(r.start_radius), f64::from(r.end_radius));
            if !finite(r.start_center) || !finite(r.end_center) || !r0.is_finite() || !r1.is_finite() {
                return Err(first);
            }
            let same_center = (r.start_center - r.end_center).hypot() <= EPS;
            if r0 < 0.0 || r1 < 0.0 || (same_center && (r0 - r1).abs() <= EPS) {
                return Err(first);
            }
        }
        GradientKind::Sweep(s) => {
            if !finite(s.center) || !s.start_angle.is_finite() || !s.end_angle.is_finite() {
                return Err(first);
            }
            if f64::from(s.end_angle) - f64::from(s.start_angle) <= EPS {
                return Err(first);
            }
        }
    }
    if !paint_xf.is_finite() || paint_xf.determinant().abs() < 1e-12 {
        return Err(first);
    }
    Ok(())
}

// ---- §5.2 ramps ----

/// One linear piece of a ramp: color(t) = bias + t × scale for t up to `x1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RampRange {
    pub(crate) x1: f32,
    pub(crate) bias: [f32; 4],
    pub(crate) scale: [f32; 4],
    /// Interpolated unpremultiplied; premultiply when baking.
    pub(crate) unpremultiplied: bool,
}

fn endpoint(c: AlphaColor<Srgb>, alpha_space: InterpolationAlphaSpace) -> [f32; 4] {
    match alpha_space {
        InterpolationAlphaSpace::Premultiplied => c.premultiply().components,
        _ => c.components,
    }
}

/// §5.2 steps 1–4 (stops already validated).
#[allow(clippy::indexing_slicing)] // `from_fn` indices are < 4 on [f32; 4]
pub(crate) fn encode_ramp_ranges(
    stops: &[ColorStop],
    cs: ColorSpaceTag,
    hue: HueDirection,
    alpha_space: InterpolationAlphaSpace,
    scratch: &mut Vec<(f32, [f32; 4])>,
    out: &mut Vec<RampRange>,
) {
    out.clear();
    scratch.clear();
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else { return };
    let mut padded: Vec<ColorStop> = Vec::with_capacity(stops.len() + 2);
    if first.offset > 0.0 {
        padded.push(ColorStop { offset: 0.0, color: first.color });
    }
    padded.extend_from_slice(stops);
    if last.offset < 1.0 {
        padded.push(ColorStop { offset: 1.0, color: last.color });
    }
    for (&s0, &s1) in padded.iter().zip(padded.iter().skip(1)) {
        let (o0, o1) = (s0.offset, s1.offset);
        if cs == ColorSpaceTag::Srgb {
            push_point(scratch, (o0, endpoint(s0.color.to_alpha_color::<Srgb>(), alpha_space)));
            push_point(scratch, (o1, endpoint(s1.color.to_alpha_color::<Srgb>(), alpha_space)));
            continue;
        }
        // Sample the color crate's interpolator uniformly (exact at every sub-stop; the
        // adaptive `color::gradient` iterator under-samples near the ends of curved paths).
        let map = |t: f32| o0 + (o1 - o0) * t;
        for k in 0..=SUBSTOPS {
            let t = k as f32 / SUBSTOPS as f32;
            let c = if alpha_space == InterpolationAlphaSpace::Premultiplied {
                s0.color
                    .interpolate(s1.color, cs, hue)
                    .eval(t)
                    .to_alpha_color::<Srgb>()
                    .premultiply()
                    .components
            } else {
                s0.color
                    .interpolate_unpremultiplied(s1.color, cs, hue)
                    .eval(t)
                    .to_alpha_color::<Srgb>()
                    .components
            };
            push_point(scratch, (map(t), c));
        }
    }
    for e in scratch.iter_mut() {
        e.1 = e.1.map(|v| v.clamp(0.0, 1.0));
    }
    let unpremultiplied = alpha_space != InterpolationAlphaSpace::Premultiplied;
    for (&(x0, a), &(x1, b)) in scratch.iter().zip(scratch.iter().skip(1)) {
        let dx = (x1 - x0).max(1e-7);
        let scale: [f32; 4] = std::array::from_fn(|i| (b[i] - a[i]) / dx);
        let bias: [f32; 4] = std::array::from_fn(|i| a[i] - x0 * scale[i]);
        out.push(RampRange { x1, bias, scale, unpremultiplied });
    }
}

/// Append a ramp endpoint, dropping an exact repeat of the previous one (the shared joint of
/// two consecutive stop pairs); a hard stop (same offset, different color) is kept.
fn push_point(points: &mut Vec<(f32, [f32; 4])>, p: (f32, [f32; 4])) {
    if points.last() != Some(&p) {
        points.push(p);
    }
}

/// §5.2 `lut_len`: 256 / 512 / 1024 by range count, raised so the narrowest range gets at
/// least one texel, capped at `MAX_LUT_LEN`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // positive, ≤ MAX_LUT_LEN
pub(crate) fn lut_len(ranges: &[RampRange]) -> u32 {
    let base = match ranges.len() {
        0 | 1 => 256,
        2 => 512,
        _ => 1024,
    };
    let mut prev = 0.0f32;
    let mut need = 0u32;
    for r in ranges {
        let gap = r.x1 - prev;
        prev = r.x1;
        let n = if gap > 0.0 { (1.0 / gap).ceil().min(MAX_LUT_LEN as f32) as u32 } else { MAX_LUT_LEN };
        need = need.max(n.next_power_of_two());
    }
    base.max(need).min(MAX_LUT_LEN)
}

/// §5.2 `bake_ramp`: `len` premultiplied texels in `[0, 1]` (stored as f16 in the LUT so
/// the §5.8 dither has sub-LSB precision to work with).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::indexing_slicing
)] // x1 ∈ [0, 1] → ≤ len; indices < 4
pub(crate) fn bake_ramp(ranges: &[RampRange], len: u32, out: &mut Vec<[f32; 4]>) {
    out.clear();
    let mut start = 0u32;
    let denom = (len.max(2) - 1) as f32;
    for r in ranges {
        let end = ((r.x1 * len as f32).floor() as u32).min(len);
        for idx in start..end {
            let t = idx as f32 / denom;
            let mut c: [f32; 4] = std::array::from_fn(|i| r.bias[i] + t * r.scale[i]);
            if r.unpremultiplied {
                let a = c[3];
                for v in &mut c[..3] {
                    *v *= a;
                }
            }
            let a = c[3].clamp(0.0, 1.0);
            out.push([c[0].clamp(0.0, a), c[1].clamp(0.0, a), c[2].clamp(0.0, a), a]);
        }
        start = start.max(end);
    }
    // Float rounding can leave the last texel(s) unwritten: hold the end color.
    while out.len() < len as usize {
        let last = out.last().copied().unwrap_or([0.0; 4]);
        out.push(last);
    }
}

/// The LUT cache key (§5.2): full bit equality, no collisions.
#[derive(Clone, Debug)]
pub(crate) struct RampKey {
    pub(crate) stops: peniko::ColorStops,
    pub(crate) cs: ColorSpaceTag,
    /// `HueDirection as u8` (no Hash/Eq derive).
    pub(crate) hue: u8,
    pub(crate) alpha_space: InterpolationAlphaSpace,
}

impl RampKey {
    pub(crate) fn of(g: &Gradient) -> Self {
        Self {
            stops: g.stops.clone(),
            cs: g.interpolation_cs,
            hue: g.hue_direction as u8,
            alpha_space: g.interpolation_alpha_space,
        }
    }

    /// Bake this ramp into `out`; returns its length in texels.
    pub(crate) fn bake(&self, out: &mut Vec<[f32; 4]>) -> u32 {
        let hue = hue_of(self.hue);
        let (mut scratch, mut ranges) = (Vec::new(), Vec::new());
        encode_ramp_ranges(&self.stops, self.cs, hue, self.alpha_space, &mut scratch, &mut ranges);
        let len = lut_len(&ranges);
        bake_ramp(&ranges, len, out);
        len
    }
}

fn hue_of(v: u8) -> HueDirection {
    [HueDirection::Shorter, HueDirection::Longer, HueDirection::Increasing, HueDirection::Decreasing]
        .into_iter()
        .find(|h| *h as u8 == v)
        .unwrap_or_default()
}

impl std::hash::Hash for RampKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        peniko::color::cache_key::CacheKey(&self.stops).hash(state);
        self.cs.hash(state);
        self.hue.hash(state);
        self.alpha_space.hash(state);
    }
}

impl PartialEq for RampKey {
    fn eq(&self, other: &Self) -> bool {
        use peniko::color::cache_key::BitEq;
        self.stops.bit_eq(&other.stops)
            && self.cs == other.cs
            && self.hue == other.hue
            && self.alpha_space == other.alpha_space
    }
}

impl Eq for RampKey {}

// ---- §5.4 geometry ----

/// The orientation-preserving similarity mapping `p0 → (0, 0)` and `p1 → (1, 0)`.
///
/// Read points as complex numbers: `z ↦ (z − p0) / (p1 − p0)`; dividing by `w = u + iv` is
/// multiplying by `conj(w) / |w|²`.
pub(crate) fn line_to_unit(p0: Point, p1: Point) -> Affine {
    let w = p1 - p0;
    let inv = 1.0 / w.hypot2();
    let (re, im) = (w.x * inv, -w.y * inv);
    // (x + iy)·(re + i·im) = (re·x − im·y) + i(im·x + re·y)
    Affine::new([re, im, -im, re, 0.0, 0.0]) * Affine::translate(-p0.to_vec2())
}

/// The CPU part of a gradient draw (§5.4); the executor adds the ramp's LUT placement.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct GradientGeom {
    pub(crate) kind: u32,
    pub(crate) extend: u32,
    /// `G` rows: device px → gradient space.
    pub(crate) m0: [f32; 4],
    pub(crate) m1: [f32; 4],
    pub(crate) p0: [f32; 4],
    pub(crate) p1: [f32; 4],
}

/// Gradient kinds (`info.x`). Kind 5 (the LUT-full solid) is assigned by the executor.
pub(crate) const KIND_LINEAR: u32 = 0;
pub(crate) const KIND_RADIAL_CONCENTRIC: u32 = 1;
pub(crate) const KIND_RADIAL_STRIP: u32 = 2;
pub(crate) const KIND_RADIAL_TWO_CIRCLE: u32 = 3;
pub(crate) const KIND_SWEEP: u32 = 4;

/// §5.4 for a validated gradient under paint transform `paint_xf`.
pub(crate) fn gradient_geom(g: &Gradient, paint_xf: Affine) -> GradientGeom {
    let (kind, base, p0) = match g.kind {
        GradientKind::Linear(l) => (KIND_LINEAR, line_to_unit(l.start, l.end), [0.0; 4]),
        GradientKind::Radial(r) => {
            radial(r.start_center, f64::from(r.start_radius), r.end_center, f64::from(r.end_radius))
        }
        GradientKind::Sweep(s) => {
            let (a0, a1) = (f64::from(s.start_angle), f64::from(s.end_angle));
            let p0 = [narrow_f32(a0), narrow_f32(1.0 / (a1 - a0)), 0.0, 0.0];
            (KIND_SWEEP, Affine::translate(-s.center.to_vec2()), p0)
        }
    };
    let m = (base * paint_xf.inverse()).as_coeffs().map(narrow_f32);
    GradientGeom {
        kind,
        extend: g.extend as u32,
        m0: [m[0], m[2], m[4], 0.0],
        m1: [m[1], m[3], m[5], 0.0],
        p0,
        p1: [0.0; 4],
    }
}

/// The radial kinds (module docs). `p0` per kind:
/// - concentric: `(−r0 / (r1 − r0), sign(r1 − r0), 0, 0)` — `t = p0.x + p0.y·|q|`;
/// - strip / two-circle: `(ρ0, Δ, 1 − Δ², 0)` — the quadratic's coefficients.
fn radial(c0: Point, r0: f64, c1: Point, r1: f64) -> (u32, Affine, [f32; 4]) {
    let dist = (c1 - c0).hypot();
    let dr = r1 - r0;
    if dist <= TOL && dr.abs() > EPS {
        // One center: the circles are |q| = r0 + ω·dr.
        let base = Affine::scale(1.0 / dr.abs()) * Affine::translate(-c0.to_vec2());
        let p0 = [narrow_f32(-r0 / dr), narrow_f32(dr.signum()), 0.0, 0.0];
        return (KIND_RADIAL_CONCENTRIC, base, p0);
    }
    let (rho0, delta) = (r0 / dist, dr / dist);
    let kind = if dr.abs() <= TOL { KIND_RADIAL_STRIP } else { KIND_RADIAL_TWO_CIRCLE };
    // When one circle touches the other from inside (|Δ| = 1) the quadratic degenerates to a
    // linear equation; snapping a within-tolerance leading coefficient to exactly 0 keeps the
    // f32 shader from inventing a spurious far root on that boundary.
    let a = 1.0 - delta * delta;
    let a = if a.abs() <= TOL { 0.0 } else { a };
    let p0 = [narrow_f32(rho0), narrow_f32(delta), narrow_f32(a), 0.0];
    (kind, line_to_unit(c0, c1), p0)
}

#[cfg(test)]
#[path = "gradient_tests.rs"]
mod tests;
