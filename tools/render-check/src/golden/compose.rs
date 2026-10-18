//! Mix and Compose (09 §4, V45), written from W3C *Compositing and Blending Level 1* in
//! premultiplied sRGB-encoded `f64` (DR-05) — never from the shaders.

use peniko::{Compose, Mix};

fn lum(c: [f64; 3]) -> f64 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn clip_color(c: [f64; 3]) -> [f64; 3] {
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

fn set_lum(c: [f64; 3], l: f64) -> [f64; 3] {
    let d = l - lum(c);
    clip_color(c.map(|v| v + d))
}

fn sat(c: [f64; 3]) -> f64 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

/// W3C SetSat: the max channel becomes `s`, the min 0, the mid scaled proportionally.
fn set_sat(c: [f64; 3], s: f64) -> [f64; 3] {
    let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2]));
    if mx > mn { c.map(|v| (v - mn) * s / (mx - mn)) } else { [0.0; 3] }
}

fn hard_light(cb: f64, cs: f64) -> f64 {
    if cs <= 0.5 { cb * 2.0 * cs } else { screen(cb, 2.0 * cs - 1.0) }
}

fn screen(cb: f64, cs: f64) -> f64 {
    cb + cs - cb * cs
}

fn soft_light(cb: f64, cs: f64) -> f64 {
    if cs <= 0.5 {
        cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
    } else {
        let d = if cb <= 0.25 { ((16.0 * cb - 12.0) * cb + 4.0) * cb } else { cb.sqrt() };
        cb + (2.0 * cs - 1.0) * (d - cb)
    }
}

/// `B(Cb, Cs)` for unpremultiplied colors.
pub fn blend(mix: Mix, cb: [f64; 3], cs: [f64; 3]) -> [f64; 3] {
    let per = |f: fn(f64, f64) -> f64| [f(cb[0], cs[0]), f(cb[1], cs[1]), f(cb[2], cs[2])];
    match mix {
        Mix::Multiply => per(|b, s| b * s),
        Mix::Screen => per(screen),
        Mix::Overlay => per(|b, s| hard_light(s, b)),
        Mix::Darken => per(f64::min),
        Mix::Lighten => per(f64::max),
        Mix::ColorDodge => per(|b, s| {
            if b == 0.0 {
                0.0
            } else if s >= 1.0 {
                1.0
            } else {
                (b / (1.0 - s)).min(1.0)
            }
        }),
        Mix::ColorBurn => per(|b, s| {
            if b >= 1.0 {
                1.0
            } else if s <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - b) / s).min(1.0)
            }
        }),
        Mix::HardLight => per(hard_light),
        Mix::SoftLight => per(soft_light),
        Mix::Difference => per(|b, s| (b - s).abs()),
        Mix::Exclusion => per(|b, s| b + s - 2.0 * b * s),
        Mix::Hue => set_lum(set_sat(cs, sat(cb)), lum(cb)),
        Mix::Saturation => set_lum(set_sat(cb, sat(cs)), lum(cb)),
        Mix::Color => set_lum(cs, lum(cb)),
        Mix::Luminosity => set_lum(cb, lum(cs)),
        // Normal, and any future mode, is the source.
        _ => cs,
    }
}

/// Porter-Duff `(Fa, Fb)` for `αs`, `αb`.
fn factors(compose: Compose, a_s: f64, a_b: f64) -> (f64, f64) {
    match compose {
        Compose::Clear => (0.0, 0.0),
        Compose::Copy => (1.0, 0.0),
        Compose::Dest => (0.0, 1.0),
        Compose::DestOver => (1.0 - a_b, 1.0),
        Compose::SrcIn => (a_b, 0.0),
        Compose::DestIn => (0.0, a_s),
        Compose::SrcOut => (1.0 - a_b, 0.0),
        Compose::DestOut => (0.0, 1.0 - a_s),
        Compose::SrcAtop => (a_b, 1.0 - a_s),
        Compose::DestAtop => (1.0 - a_b, a_s),
        Compose::Xor => (1.0 - a_b, 1.0 - a_s),
        Compose::Plus | Compose::PlusLighter => (1.0, 1.0),
        // SrcOver and any future mode.
        _ => (1.0, 1.0 - a_s),
    }
}

/// Composite premultiplied source `s` onto premultiplied backdrop `b`.
pub fn composite(b: [f64; 4], s: [f64; 4], mix: Mix, compose: Compose) -> [f64; 4] {
    let unpremul = |c: [f64; 4]| {
        if c[3] > 0.0 { [c[0] / c[3], c[1] / c[3], c[2] / c[3]] } else { [0.0; 3] }
    };
    let (cs, cb) = (unpremul(s), unpremul(b));
    let m = blend(mix, cb, cs);
    // The mixed source: blended where the backdrop is opaque.
    let cs2: [f64; 3] = std::array::from_fn(|i| (1.0 - b[3]) * cs[i] + b[3] * m[i]);
    let (fa, fb) = factors(compose, s[3], b[3]);
    let mut out = [0.0; 4];
    for i in 0..3 {
        out[i] = s[3] * fa * cs2[i] + b[3] * fb * cb[i];
    }
    out[3] = s[3] * fa + b[3] * fb;
    if matches!(compose, Compose::Plus | Compose::PlusLighter) {
        out = out.map(|v| v.min(1.0));
    }
    out[3] = out[3].min(1.0);
    out
}
