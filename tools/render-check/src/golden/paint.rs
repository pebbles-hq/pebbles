//! Brushes evaluated at device pixel centers (V45): solid, gradients (09 §5 model, DR-05
//! interpolation) and images (02 §8, 09 §6), in premultiplied sRGB-encoded `f64`. Derived
//! from the spec text and the gradient definitions (CSS/canvas), never from the shaders.

use kurbo::{Affine, Point, Vec2};
use peniko::color::{ColorSpaceTag, DynamicColor, Srgb};
use peniko::{Brush, Extend, Gradient, GradientKind, ImageAlphaType, ImageBrush, ImageFormat, ImageQuality};

use crate::CheckError;

/// A brush ready to evaluate at device pixel centers.
pub enum Paint {
    Solid([f64; 4]),
    Gradient {
        inv: Affine,
        kind: GradientKind,
        extend: Extend,
        ramp: Ramp,
        alpha: f64,
    },
    Image {
        inv: Affine,
        img: Texels,
        x_extend: Extend,
        y_extend: Extend,
        nearest: bool,
        alpha: f64,
    },
    /// A shader brush (09 §6.1, L43) judged through its corpus oracle — the Rust mirror of the
    /// case's WGSL (the renderer never evaluates WGSL on the CPU).
    Shader {
        inv: Affine,
        oracle: ShaderOracle,
        values: Vec<f32>,
        size: (f64, f64),
    },
    /// A minified `Medium`/`High` image (R35): the mip chain and the per-draw LOD.
    Mipped {
        inv: Affine,
        levels: Vec<Texels>,
        x_extend: Extend,
        y_extend: Extend,
        lod: f64,
        trilinear: bool,
        alpha: f64,
    },
}

/// The CPU mirror of a corpus shader program: `(local, device, u.values, u.size)` →
/// premultiplied RGBA.
pub type ShaderOracle = fn(Point, Point, &[f32], (f64, f64)) -> [f64; 4];

fn premul_color(c: peniko::Color) -> [f64; 4] {
    let [r, g, b, a] = c.components.map(f64::from);
    [r * a, g * a, b * a, a]
}

impl Paint {
    /// `brush` under paint transform `p` (device ← brush space) with extra `alpha`. `None` =
    /// draw nothing (degenerate image brush, 09 §6).
    pub fn new(brush: &Brush, p: Affine, alpha: f64) -> Result<Option<Paint>, CheckError> {
        match brush {
            Brush::Solid(c) => Ok(Some(Paint::Solid(premul_color(*c).map(|v| v * alpha)))),
            Brush::Gradient(g) => Ok(Some(gradient(g, p, alpha)?)),
            Brush::Image(i) => Ok(image(i, p, alpha)),
        }
    }

    /// Premultiplied color at device pixel center `(x, y)`.
    /// Gradients dither (09 §5.8); solids and images never do.
    pub fn dithers(&self) -> bool {
        matches!(self, Paint::Gradient { .. })
    }

    pub fn at(&self, x: f64, y: f64) -> [f64; 4] {
        match self {
            Paint::Solid(c) => *c,
            Paint::Gradient { inv, kind, extend, ramp, alpha } => {
                let g = *inv * Point::new(x, y);
                match gradient_t(kind, g) {
                    Some(t) => ramp.at(apply_extend(t, *extend)).map(|v| v * alpha),
                    None => [0.0; 4],
                }
            }
            Paint::Image { inv, img, x_extend, y_extend, nearest, alpha } => {
                let uv = *inv * Point::new(x, y);
                img.sample(uv.x, uv.y, *x_extend, *y_extend, *nearest).map(|v| v * alpha)
            }
            Paint::Shader { inv, oracle, values, size } => {
                let l = *inv * Point::new(x, y);
                oracle(l, Point::new(x, y), values, *size).map(|v| v.clamp(0.0, 1.0))
            }
            Paint::Mipped { inv, levels, x_extend, y_extend, lod, trilinear, alpha } => {
                let uv = *inv * Point::new(x, y);
                let at = |l: usize| {
                    let (base, lv) = (&levels[0], &levels[l.min(levels.len() - 1)]);
                    let (sx, sy) = (f64::from(lv.w) / f64::from(base.w), f64::from(lv.h) / f64::from(base.h));
                    lv.sample(uv.x * sx, uv.y * sy, *x_extend, *y_extend, false)
                };
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // lod ≥ 0, small
                let c = if *trilinear {
                    let (l0, f) = (lod.floor(), lod - lod.floor());
                    let (a, b) = (at(l0 as usize), at(l0 as usize + 1));
                    std::array::from_fn(|k| a[k] * (1.0 - f) + b[k] * f)
                } else {
                    at(lod.round() as usize)
                };
                c.map(|v| v * alpha)
            }
        }
    }
}

/// 09 §5.5 extend modes on `t`.
/// 09 §5.8 (L37): the GPU's ordered 8×8 Bayer table.
pub const BAYER8: [u8; 64] = [
    0, 32, 8, 40, 2, 34, 10, 42, 48, 16, 56, 24, 50, 18, 58, 26, //
    12, 44, 4, 36, 14, 46, 6, 38, 60, 28, 52, 20, 62, 30, 54, 22, //
    3, 35, 11, 43, 1, 33, 9, 41, 51, 19, 59, 27, 49, 17, 57, 25, //
    15, 47, 7, 39, 13, 45, 5, 37, 63, 31, 55, 23, 61, 29, 53, 21,
];

/// The §5.8 dither of a premultiplied gradient contribution at device pixel `(x, y)`, in f32
/// like the shader; alpha untouched, rgb clamped to `[0, a]`.
pub fn dither(c: [f64; 4], x: usize, y: usize) -> [f64; 4] {
    let b = f32::from(BAYER8[(y % 8) * 8 + x % 8]);
    let d = (b - 31.5) / (64.0 * 255.0);
    let a = c[3] as f32;
    let ch = |v: f64| f64::from((v as f32 + d).clamp(0.0, a));
    [ch(c[0]), ch(c[1]), ch(c[2]), c[3]]
}

pub fn apply_extend(t: f64, e: Extend) -> f64 {
    match e {
        Extend::Repeat => t - t.floor(),
        Extend::Reflect => {
            let m = t.rem_euclid(2.0);
            if m > 1.0 { 2.0 - m } else { m }
        }
        _ => t.clamp(0.0, 1.0),
    }
}

/// The gradient parameter at gradient-space point `g`; `None` = no color (no conical solution).
fn gradient_t(kind: &GradientKind, g: Point) -> Option<f64> {
    match kind {
        GradientKind::Linear(l) => {
            let d = l.end - l.start;
            Some((g - l.start).dot(d) / d.hypot2())
        }
        GradientKind::Radial(r) => {
            conical_t(r.start_center, f64::from(r.start_radius), r.end_center, f64::from(r.end_radius), g)
        }
        GradientKind::Sweep(s) => {
            let v = g - s.center;
            let mut a = v.y.atan2(v.x);
            if a < 0.0 {
                a += std::f64::consts::TAU;
            }
            let (a0, a1) = (f64::from(s.start_angle), f64::from(s.end_angle));
            Some((a - a0) / (a1 - a0))
        }
    }
}

/// Two-point conical gradient (canvas/CSS definition): the largest `t` with
/// `|g − c(t)| = r(t)` and `r(t) ≥ 0`, `c(t) = c0 + t(c1 − c0)`, `r(t) = r0 + t(r1 − r0)`.
pub fn conical_t(c0: Point, r0: f64, c1: Point, r1: f64, g: Point) -> Option<f64> {
    let dc: Vec2 = c1 - c0;
    let dr = r1 - r0;
    let q: Vec2 = g - c0;
    let a = dc.hypot2() - dr * dr;
    let b = q.dot(dc) + r0 * dr;
    let c = q.hypot2() - r0 * r0;
    let ok = |t: f64| r0 + t * dr >= 0.0;
    if a.abs() < 1e-12 {
        if b.abs() < 1e-12 {
            return None;
        }
        let t = c / (2.0 * b);
        return ok(t).then_some(t);
    }
    let disc = b * b - a * c;
    if disc < 0.0 {
        return None;
    }
    let s = disc.sqrt();
    let (t1, t2) = ((b + s) / a, (b - s) / a);
    let (hi, lo) = (t1.max(t2), t1.min(t2));
    if ok(hi) {
        Some(hi)
    } else if ok(lo) {
        Some(lo)
    } else {
        None
    }
}

/// A color ramp over `[0, 1]`: premultiplied sRGB at `t`.
pub enum Ramp {
    /// sRGB stops; `premul` = interpolate premultiplied (else straight, premultiplied after).
    Srgb { stops: Vec<(f64, [f64; 4])>, premul: bool },
    /// Another interpolation space (DR-05): the color crate's interpolator per segment.
    Space {
        stops: Vec<(f64, DynamicColor)>,
        cs: ColorSpaceTag,
        hue: peniko::color::HueDirection,
        premul: bool,
    },
}

impl Ramp {
    fn at(&self, t: f64) -> [f64; 4] {
        match self {
            Ramp::Srgb { stops, premul } => {
                let (i, u) = segment(stops.iter().map(|s| s.0), t);
                let (a, b) = (stops[i].1, stops[(i + 1).min(stops.len() - 1)].1);
                let c: [f64; 4] = std::array::from_fn(|k| a[k] + (b[k] - a[k]) * u);
                if *premul { c } else { [c[0] * c[3], c[1] * c[3], c[2] * c[3], c[3]] }
            }
            Ramp::Space { stops, cs, hue, premul } => {
                let (i, u) = segment(stops.iter().map(|s| s.0), t);
                let (a, b) = (stops[i].1, stops[(i + 1).min(stops.len() - 1)].1);
                #[allow(clippy::cast_possible_truncation)] // interpolation parameter in [0, 1]
                let c = if *premul {
                    a.interpolate(b, *cs, *hue).eval(u as f32)
                } else {
                    a.interpolate_unpremultiplied(b, *cs, *hue).eval(u as f32)
                };
                premul_color(c.to_alpha_color::<Srgb>())
            }
        }
    }
}

/// The segment index `i` (stops `i`, `i + 1`) containing `t` and the local parameter.
fn segment(offsets: impl Iterator<Item = f64>, t: f64) -> (usize, f64) {
    let o: Vec<f64> = offsets.collect();
    for i in 0..o.len().saturating_sub(1) {
        if t <= o[i + 1] {
            let span = o[i + 1] - o[i];
            return (i, if span > 0.0 { ((t - o[i]) / span).clamp(0.0, 1.0) } else { 1.0 });
        }
    }
    (o.len().saturating_sub(2), 1.0)
}

fn gradient(g: &Gradient, p: Affine, alpha: f64) -> Result<Paint, CheckError> {
    let stops = &g.stops.0;
    let first = || stops.first().map_or(peniko::Color::BLACK, |s| s.color.to_alpha_color::<Srgb>());
    let solid = |c: peniko::Color| Paint::Solid(premul_color(c).map(|v| v * alpha));
    // 09 §5.1 validation: a degenerate gradient draws as a solid.
    if stops.is_empty() {
        return Ok(solid(peniko::Color::BLACK));
    }
    let offsets_ok = stops.iter().all(|s| (0.0..=1.0).contains(&s.offset))
        && stops.windows(2).all(|w| w[0].offset <= w[1].offset);
    let geometry_ok = match &g.kind {
        GradientKind::Linear(l) => {
            (l.end - l.start).hypot() > 1e-6 && l.start.is_finite() && l.end.is_finite()
        }
        GradientKind::Radial(r) => {
            let (r0, r1) = (f64::from(r.start_radius), f64::from(r.end_radius));
            r0 >= 0.0
                && r1 >= 0.0
                && r0.is_finite()
                && r1.is_finite()
                && r.start_center.is_finite()
                && r.end_center.is_finite()
                && !((r.end_center - r.start_center).hypot() <= 1e-6 && (r1 - r0).abs() <= 1e-6)
        }
        GradientKind::Sweep(s) => {
            s.center.is_finite()
                && s.start_angle.is_finite()
                && s.end_angle.is_finite()
                && s.end_angle - s.start_angle > 1e-6
        }
    };
    if stops.len() == 1 || !offsets_ok || !geometry_ok || !p.is_finite() || p.determinant().abs() < 1e-12 {
        return Ok(solid(first()));
    }
    // Pad the stop list to [0, 1].
    let mut padded: Vec<(f64, DynamicColor)> = stops.iter().map(|s| (f64::from(s.offset), s.color)).collect();
    if padded[0].0 > 0.0 {
        padded.insert(0, (0.0, padded[0].1));
    }
    if let Some(&(o, c)) = padded.last()
        && o < 1.0
    {
        padded.push((1.0, c));
    }
    let premul = g.interpolation_alpha_space == peniko::InterpolationAlphaSpace::Premultiplied;
    let ramp = if g.interpolation_cs == ColorSpaceTag::Srgb {
        let stops = padded
            .into_iter()
            .map(|(o, c)| {
                let c = if premul {
                    premul_color(c.to_alpha_color::<Srgb>())
                } else {
                    // Straight components: a transparent stop keeps its color (peniko
                    // `InterpolationAlphaSpace::Unpremultiplied`).
                    c.to_alpha_color::<Srgb>().components.map(f64::from)
                };
                (o, c)
            })
            .collect();
        Ramp::Srgb { stops, premul }
    } else {
        Ramp::Space { stops: padded, cs: g.interpolation_cs, hue: g.hue_direction, premul }
    };
    Ok(Paint::Gradient { inv: p.inverse(), kind: g.kind, extend: g.extend, ramp, alpha })
}

/// Premultiplied `f64` texels of an image.
pub struct Texels {
    pub w: u32,
    pub h: u32,
    pub px: Vec<[f64; 4]>,
}

impl Texels {
    /// Decode `ImageData` (Rgba8/Bgra8, straight or premultiplied) to premultiplied `f64`.
    pub fn of(data: &peniko::ImageData) -> Texels {
        let bytes = data.data.data();
        let px = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| {
                let mut c = [p[0], p[1], p[2], p[3]].map(|v| f64::from(v) / 255.0);
                if data.format == ImageFormat::Bgra8 {
                    c.swap(0, 2);
                }
                if data.alpha_type == ImageAlphaType::Alpha {
                    for i in 0..3 {
                        c[i] *= c[3];
                    }
                }
                c
            })
            .collect();
        Texels { w: data.width, h: data.height, px }
    }

    fn texel(&self, i: i64, j: i64, xe: Extend, ye: Extend) -> [f64; 4] {
        let wrap = |k: i64, n: u32, e: Extend| -> usize {
            let n = i64::from(n);
            let k = match e {
                Extend::Repeat => k.rem_euclid(n),
                Extend::Reflect => {
                    let m = k.rem_euclid(2 * n);
                    if m >= n { 2 * n - 1 - m } else { m }
                }
                _ => k.clamp(0, n - 1),
            };
            usize::try_from(k).unwrap_or(0)
        };
        self.px[wrap(j, self.h, ye) * self.w as usize + wrap(i, self.w, xe)]
    }

    /// Sample at image-pixel coordinates `(u, v)`: nearest = the texel containing the point,
    /// else bilinear between texel centers (02 §8, 09 §6).
    #[allow(clippy::cast_possible_truncation)] // floor of a finite coordinate
    pub fn sample(&self, u: f64, v: f64, xe: Extend, ye: Extend, nearest: bool) -> [f64; 4] {
        if !(u.is_finite() && v.is_finite()) || self.w == 0 || self.h == 0 {
            return [0.0; 4];
        }
        if nearest {
            return self.texel(u.floor() as i64, v.floor() as i64, xe, ye);
        }
        let (x, y) = (u - 0.5, v - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let (i, j) = (x0 as i64, y0 as i64);
        let t = |di, dj| self.texel(i + di, j + dj, xe, ye);
        let (a, b, c, d) = (t(0, 0), t(1, 0), t(0, 1), t(1, 1));
        std::array::from_fn(|k| {
            (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy
        })
    }
}

/// R35 LOD: with `L` the 2×2 linear part of the image → device map, `ρ = 1 / σ_min(L)`
/// (texels per device px along the most minified axis), `lod = max(0, log2 ρ)`.
pub fn lod(p: Affine) -> f64 {
    let [a, b, c, d, ..] = p.as_coeffs();
    // Singular values of [[a, c], [b, d]].
    let s1 = a * a + b * b + c * c + d * d;
    let det = (a * d - b * c).abs();
    let disc = (s1 * s1 - 4.0 * det * det).max(0.0).sqrt();
    let smin = ((s1 - disc) / 2.0).max(0.0).sqrt();
    if smin <= 0.0 { f64::INFINITY } else { (1.0 / smin).log2().max(0.0) }
}

/// R35 mip chain: each level halves each side (`max(1, n / 2)`); a texel is
/// `(a + b + c + d + 2) >> 2` per channel of its 2×2 block on premultiplied RGBA8 (odd sizes
/// clamp to edge). Level 0 is `base` itself.
pub fn mip_chain(base: &Texels) -> Vec<Texels> {
    let q = |v: f64| -> u32 {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..=255
        let v = (v.clamp(0.0, 1.0) * 255.0).round() as u32;
        v
    };
    let mut levels = vec![Texels { w: base.w, h: base.h, px: base.px.clone() }];
    let mut cur: Vec<[u32; 4]> = base.px.iter().map(|p| p.map(q)).collect();
    let (mut w, mut h) = (base.w, base.h);
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let at = |x: u32, y: u32| cur[(y.min(h - 1) * w + x.min(w - 1)) as usize];
        let next: Vec<[u32; 4]> = (0..nh)
            .flat_map(|y| (0..nw).map(move |x| (x, y)))
            .map(|(x, y)| {
                let (a, b, c, d) =
                    (at(2 * x, 2 * y), at(2 * x + 1, 2 * y), at(2 * x, 2 * y + 1), at(2 * x + 1, 2 * y + 1));
                std::array::from_fn(|k| (a[k] + b[k] + c[k] + d[k] + 2) >> 2)
            })
            .collect();
        levels.push(Texels {
            w: nw,
            h: nh,
            px: next.iter().map(|p| p.map(|v| f64::from(v) / 255.0)).collect(),
        });
        (cur, w, h) = (next, nw, nh);
    }
    levels
}

/// An image brush: `None` when degenerate (zero size, singular transform).
fn image(i: &ImageBrush, p: Affine, alpha: f64) -> Option<Paint> {
    if i.image.width == 0 || i.image.height == 0 || !p.is_finite() || p.determinant().abs() < 1e-12 {
        return None;
    }
    let l = lod(p);
    if i.sampler.quality != ImageQuality::Low && l > 0.0 {
        // R35: Medium = the nearest level (bilinear in it), High = trilinear.
        let levels = mip_chain(&Texels::of(&i.image));
        return Some(Paint::Mipped {
            inv: p.inverse(),
            levels,
            x_extend: i.sampler.x_extend,
            y_extend: i.sampler.y_extend,
            lod: l,
            trilinear: i.sampler.quality == ImageQuality::High,
            alpha: alpha * f64::from(i.sampler.alpha),
        });
    }
    Some(Paint::Image {
        inv: p.inverse(),
        img: Texels::of(&i.image),
        x_extend: i.sampler.x_extend,
        y_extend: i.sampler.y_extend,
        nearest: i.sampler.quality == ImageQuality::Low,
        alpha: alpha * f64::from(i.sampler.alpha),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn bayer_table_is_permutation() {
        let mut seen = [false; 64];
        for &v in &super::BAYER8 {
            assert!(!std::mem::replace(&mut seen[usize::from(v)], true), "{v} twice");
        }
        assert!(seen.iter().all(|&s| s));
    }
}
