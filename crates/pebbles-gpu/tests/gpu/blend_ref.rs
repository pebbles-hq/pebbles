//! CPU reference for W3C *Compositing and Blending Level 1* (09 §4.6, L22), written from the
//! spec text in `f64`: separable modes (§9.1.4 / §10.1), non-separable modes (§10.2) and the
//! Porter-Duff operators (§9.1). Premultiplied RGBA in and out. The GPU blend tests (L21, L25)
//! compare against it; it never shares code with the WGSL.

use peniko::{Compose, Mix};

/// W3C §10.2 `Lum`.
pub fn lum(c: [f64; 3]) -> f64 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

/// W3C §10.2 `ClipColor`.
pub fn clip_color(c: [f64; 3]) -> [f64; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut c = c;
    if n < 0.0 {
        c = c.map(|v| l + (v - l) * l / (l - n));
    }
    if x > 1.0 {
        c = c.map(|v| l + (v - l) * (1.0 - l) / (x - l));
    }
    c
}

/// W3C §10.2 `SetLum`.
pub fn set_lum(c: [f64; 3], l: f64) -> [f64; 3] {
    let d = l - lum(c);
    clip_color(c.map(|v| v + d))
}

/// W3C §10.2 `Sat`.
pub fn sat(c: [f64; 3]) -> f64 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

/// W3C §10.2 `SetSat`, in the spec's min/mid/max form.
pub fn set_sat(c: [f64; 3], s: f64) -> [f64; 3] {
    let mut idx = [0usize, 1, 2];
    idx.sort_by(|&a, &b| c[a].total_cmp(&c[b]));
    let (min, mid, max) = (idx[0], idx[1], idx[2]);
    let mut out = [0.0; 3];
    if c[max] > c[min] {
        out[mid] = (c[mid] - c[min]) * s / (c[max] - c[min]);
        out[max] = s;
    }
    // out[min] (and, when max == min, every channel) stays 0.
    out
}

fn separable(cb: f64, cs: f64, mix: Mix) -> f64 {
    let multiply = |b: f64, s: f64| b * s;
    let screen = |b: f64, s: f64| b + s - b * s;
    let hard_light = |b: f64, s: f64| if s <= 0.5 { multiply(b, 2.0 * s) } else { screen(b, 2.0 * s - 1.0) };
    match mix {
        Mix::Multiply => multiply(cb, cs),
        Mix::Screen => screen(cb, cs),
        Mix::Overlay => hard_light(cs, cb),
        Mix::Darken => cb.min(cs),
        Mix::Lighten => cb.max(cs),
        Mix::ColorDodge => {
            if cb == 0.0 {
                0.0
            } else if cs >= 1.0 {
                1.0
            } else {
                (cb / (1.0 - cs)).min(1.0)
            }
        }
        Mix::ColorBurn => {
            if cb >= 1.0 {
                1.0
            } else if cs <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - cb) / cs).min(1.0)
            }
        }
        Mix::HardLight => hard_light(cb, cs),
        Mix::SoftLight => {
            if cs <= 0.5 {
                cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
            } else {
                let d = if cb <= 0.25 { ((16.0 * cb - 12.0) * cb + 4.0) * cb } else { cb.sqrt() };
                cb + (2.0 * cs - 1.0) * (d - cb)
            }
        }
        Mix::Difference => (cb - cs).abs(),
        Mix::Exclusion => cb + cs - 2.0 * cb * cs,
        _ => cs, // Normal
    }
}

/// W3C §10 `B(Cb, Cs)` for every `Mix` (non-premultiplied colors).
pub fn blend_mix(cb: [f64; 3], cs: [f64; 3], mix: Mix) -> [f64; 3] {
    match mix {
        Mix::Hue => set_lum(set_sat(cs, sat(cb)), lum(cb)),
        Mix::Saturation => set_lum(set_sat(cb, sat(cs)), lum(cb)),
        Mix::Color => set_lum(cs, lum(cb)),
        Mix::Luminosity => set_lum(cb, lum(cs)),
        _ => [0, 1, 2].map(|i| separable(cb[i], cs[i], mix)),
    }
}

/// W3C §9.1 Porter-Duff `(Fa, Fb)` for `compose` given the source / backdrop alphas.
fn factors(compose: Compose, a_s: f64, a_b: f64) -> (f64, f64) {
    match compose {
        Compose::Clear => (0.0, 0.0),
        Compose::Copy => (1.0, 0.0),
        Compose::Dest => (0.0, 1.0),
        Compose::SrcOver => (1.0, 1.0 - a_s),
        Compose::DestOver => (1.0 - a_b, 1.0),
        Compose::SrcIn => (a_b, 0.0),
        Compose::DestIn => (0.0, a_s),
        Compose::SrcOut => (1.0 - a_b, 0.0),
        Compose::DestOut => (0.0, 1.0 - a_s),
        Compose::SrcAtop => (a_b, 1.0 - a_s),
        Compose::DestAtop => (1.0 - a_b, a_s),
        Compose::Xor => (1.0 - a_b, 1.0 - a_s),
        Compose::Plus | Compose::PlusLighter => (1.0, 1.0),
    }
}

/// W3C §5 general formula: blend the source with the backdrop (`Cs' = (1 − αb)·Cs + αb·B(Cb,
/// Cs)`), then composite with Porter-Duff: `co = αs·Fa·Cs' + αb·Fb·Cb`, `αo = αs·Fa + αb·Fb`,
/// clamped to 1. Premultiplied in and out.
pub fn composite(b: [f64; 4], s: [f64; 4], mix: Mix, compose: Compose) -> [f64; 4] {
    let unpremul = |c: [f64; 4]| {
        if c[3] > 0.0 { [c[0] / c[3], c[1] / c[3], c[2] / c[3]] } else { [0.0; 3] }
    };
    let (cs, cb) = (unpremul(s), unpremul(b));
    let m = blend_mix(cb, cs, mix);
    let cs2 = [0, 1, 2].map(|i| (1.0 - b[3]) * cs[i] + b[3] * m[i]);
    let (fa, fb) = factors(compose, s[3], b[3]);
    let rgb = [0, 1, 2].map(|i| (s[3] * fa * cs2[i] + b[3] * fb * cb[i]).min(1.0));
    let a = (s[3] * fa + b[3] * fb).min(1.0);
    [rgb[0], rgb[1], rgb[2], a]
}

/// `(v · 255 + 0.5)` clamped to a byte.
pub fn to_u8(c: [f64; 4]) -> [u8; 4] {
    c.map(|v| (v * 255.0 + 0.5).clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
mod blend_ref_spec {
    use super::*;

    fn one(mix: Mix, cb: f64, cs: f64) -> f64 {
        blend_mix([cb; 3], [cs; 3], mix)[0]
    }

    #[test]
    fn separable_values() {
        let cases = [
            (Mix::Multiply, 0.5, 0.5, 0.25),
            (Mix::Screen, 0.5, 0.5, 0.75),
            (Mix::Overlay, 0.25, 0.75, 0.375),
            (Mix::ColorDodge, 0.5, 0.5, 1.0),
            (Mix::ColorBurn, 0.5, 0.5, 0.0),
            (Mix::SoftLight, 0.25, 0.75, 0.375),
            (Mix::HardLight, 0.5, 0.25, 0.25),
            (Mix::Difference, 0.2, 0.7, 0.5),
            (Mix::Exclusion, 0.5, 0.5, 0.5),
        ];
        for (mix, cb, cs, want) in cases {
            let got = one(mix, cb, cs);
            assert!((got - want).abs() < 1e-9, "{mix:?}({cb}, {cs}) = {got}, want {want}");
        }
    }

    #[test]
    fn luminosity_red_over_green() {
        let got = blend_mix([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], Mix::Luminosity);
        let want = [1.0, 0.4143, 0.4143];
        for i in 0..3 {
            assert!((got[i] - want[i]).abs() < 1e-4, "{got:?}");
        }
    }

    #[test]
    fn porter_duff_opaque() {
        let b = [0.1, 0.2, 0.3, 1.0];
        let s = [0.7, 0.5, 0.2, 1.0];
        assert_eq!(composite(b, s, Mix::Normal, Compose::SrcIn), s);
        assert_eq!(composite(b, s, Mix::Normal, Compose::DestIn), b);
        assert_eq!(composite(b, s, Mix::Normal, Compose::Clear), [0.0; 4]);
        assert_eq!(composite(b, s, Mix::Normal, Compose::Xor)[3], 0.0);
    }

    #[test]
    fn normal_srcover_is_over() {
        let samples = [0.0, 0.4, 1.0];
        for &sa in &samples {
            for &ba in &samples {
                let s = [0.3 * sa, 0.6 * sa, 0.9 * sa, sa];
                let b = [0.8 * ba, 0.1 * ba, 0.5 * ba, ba];
                let got = composite(b, s, Mix::Normal, Compose::SrcOver);
                for i in 0..4 {
                    let want = s[i] + b[i] * (1.0 - sa);
                    assert!((got[i] - want).abs() < 1e-12, "sa {sa} ba {ba}: {got:?}");
                }
            }
        }
    }
}
