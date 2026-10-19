//! Color matrices (09 §10.5, Flutter `ColorFilter.matrix`): a row-major 4×5 matrix applied to
//! unpremultiplied RGBA in `[0, 1]`; the fifth column is an offset in `[0, 1]` units. The
//! presets are the W3C Filter Effects 1 definitions.

use super::display_list::LayerEffect;

/// A 4×5 color matrix (rows r′, g′, b′, a′ over `(r, g, b, a, 1)`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorMatrix(pub [f32; 20]);

impl ColorMatrix {
    /// From Flutter's `ColorFilter.matrix` values, whose offsets are in 0–255 units.
    pub fn from_flutter(m: [f32; 20]) -> Self {
        let mut out = m;
        for row in 0..4 {
            out[row * 5 + 4] /= 255.0;
        }
        Self(out)
    }

    fn rgb(r: [f32; 5], g: [f32; 5], b: [f32; 5]) -> Self {
        let mut m = [0.0; 20];
        m[..5].copy_from_slice(&r);
        m[5..10].copy_from_slice(&g);
        m[10..15].copy_from_slice(&b);
        m[18] = 1.0;
        Self(m)
    }

    pub fn identity() -> Self {
        Self::rgb([1.0, 0.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0, 0.0])
    }

    pub fn saturation(s: f32) -> Self {
        Self::rgb(
            [0.213 + 0.787 * s, 0.715 - 0.715 * s, 0.072 - 0.072 * s, 0.0, 0.0],
            [0.213 - 0.213 * s, 0.715 + 0.285 * s, 0.072 - 0.072 * s, 0.0, 0.0],
            [0.213 - 0.213 * s, 0.715 - 0.715 * s, 0.072 + 0.928 * s, 0.0, 0.0],
        )
    }

    pub fn grayscale() -> Self {
        Self::saturation(0.0)
    }

    pub fn sepia() -> Self {
        Self::rgb(
            [0.393, 0.769, 0.189, 0.0, 0.0],
            [0.349, 0.686, 0.168, 0.0, 0.0],
            [0.272, 0.534, 0.131, 0.0, 0.0],
        )
    }

    pub fn brightness(k: f32) -> Self {
        Self::rgb([k, 0.0, 0.0, 0.0, 0.0], [0.0, k, 0.0, 0.0, 0.0], [0.0, 0.0, k, 0.0, 0.0])
    }

    pub fn contrast(k: f32) -> Self {
        let o = (1.0 - k) / 2.0;
        Self::rgb([k, 0.0, 0.0, 0.0, o], [0.0, k, 0.0, 0.0, o], [0.0, 0.0, k, 0.0, o])
    }

    pub fn invert() -> Self {
        Self::rgb([-1.0, 0.0, 0.0, 0.0, 1.0], [0.0, -1.0, 0.0, 0.0, 1.0], [0.0, 0.0, -1.0, 0.0, 1.0])
    }

    pub fn hue_rotate(theta: f32) -> Self {
        let (s, c) = theta.sin_cos();
        Self::rgb(
            [
                0.213 + 0.787 * c - 0.213 * s,
                0.715 - 0.715 * c - 0.715 * s,
                0.072 - 0.072 * c + 0.928 * s,
                0.0,
                0.0,
            ],
            [
                0.213 - 0.213 * c + 0.143 * s,
                0.715 + 0.285 * c + 0.140 * s,
                0.072 - 0.072 * c - 0.283 * s,
                0.0,
                0.0,
            ],
            [
                0.213 - 0.213 * c - 0.787 * s,
                0.715 - 0.715 * c + 0.715 * s,
                0.072 + 0.928 * c + 0.072 * s,
                0.0,
                0.0,
            ],
        )
    }

    /// Apply to one unpremultiplied RGBA color (the §10.5 per-pixel rule, clamped).
    pub fn apply(&self, c: [f32; 4]) -> [f32; 4] {
        let m = &self.0;
        std::array::from_fn(|row| {
            let b = row * 5;
            (m[b] * c[0] + m[b + 1] * c[1] + m[b + 2] * c[2] + m[b + 3] * c[3] + m[b + 4]).clamp(0.0, 1.0)
        })
    }
}

impl From<ColorMatrix> for LayerEffect {
    fn from(m: ColorMatrix) -> Self {
        LayerEffect::ColorMatrix(m.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_flutter_scales_offsets() {
        let mut m = [0.0; 20];
        m[4] = 255.0;
        m[19] = 51.0;
        let c = ColorMatrix::from_flutter(m);
        assert_eq!((c.0[4], c.0[19]), (1.0, 0.2));
    }

    #[test]
    fn presets_match_reference_values() {
        let g = ColorMatrix::grayscale().apply([1.0, 0.0, 0.0, 1.0]);
        for v in &g[..3] {
            assert!(((v * 255.0).round() - 54.0).abs() <= 1.0, "{g:?}");
        }
        assert_eq!(ColorMatrix::invert().apply([1.0, 0.25, 0.0, 1.0]), [0.0, 0.75, 1.0, 1.0]);
        assert_eq!(ColorMatrix::identity().apply([0.2, 0.4, 0.6, 0.8]), [0.2, 0.4, 0.6, 0.8]);
    }
}
