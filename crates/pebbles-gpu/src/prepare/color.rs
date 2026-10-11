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
pub(crate) fn clear_color(color: peniko::Color) -> wgpu::Color {
    let [r, g, b, a] = premul(color);
    wgpu::Color { r: f64::from(r), g: f64::from(g), b: f64::from(b), a: f64::from(a) }
}
