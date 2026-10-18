//! The only color conversion points: premultiply a `peniko::Color` (sRGB-encoded, straight
//! alpha) into `[f32; 4]` or a `wgpu::Color` clear value.

/// Premultiply `c`'s straight alpha into its RGB (still sRGB-encoded).
pub(crate) fn premul(color: peniko::Color) -> [f32; 4] {
    let [r, g, b, a] = color.components;
    [r * a, g * a, b * a, a]
}

/// Premultiply and scale all four components by `k` (hairline alpha, layer fold, … — 02 §3.1).
pub(crate) fn premul_scaled(color: peniko::Color, k: f32) -> [f32; 4] {
    let [r, g, b, a] = premul(color);
    [r * k, g * k, b * k, a * k]
}

/// The premultiplied, sRGB-encoded clear color for `LoadOp::Clear` (02 §3.7).
/// sRGB transfer decode of one encoded channel (02 §3.8).
pub(crate) fn srgb_to_linear_f64(c: f64) -> f64 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

/// The premultiplied encoded clear color decoded to premultiplied linear for a GL sRGB-only
/// target (R13): `rgb / a` decoded channel-wise, multiplied back by `a`; `a == 0` stays zero.
pub(crate) fn linearize_clear(c: wgpu::Color) -> wgpu::Color {
    if c.a <= 0.0 {
        return wgpu::Color::TRANSPARENT;
    }
    let f = |v: f64| srgb_to_linear_f64(v / c.a) * c.a;
    wgpu::Color { r: f(c.r), g: f(c.g), b: f(c.b), a: c.a }
}

pub(crate) fn clear_color(color: peniko::Color) -> wgpu::Color {
    let [r, g, b, a] = premul(color);
    wgpu::Color { r: f64::from(r), g: f64::from(g), b: f64::from(b), a: f64::from(a) }
}
