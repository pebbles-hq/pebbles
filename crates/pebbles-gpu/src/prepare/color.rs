//! The only color conversion point: premultiply a `peniko::Color` (sRGB-encoded, straight
//! alpha) into `[f32; 4]`.

/// Premultiply `c`'s straight alpha into its RGB (still sRGB-encoded).
pub(crate) fn premul(color: peniko::Color) -> [f32; 4] {
    let [r, g, b, a] = color.components;
    [r * a, g * a, b * a, a]
}
