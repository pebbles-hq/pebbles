//! Clips and layers (09 §2–§3, V45): every `PushLayer` is an isolated `f64` buffer over
//! transparent, composited into its parent on pop with group alpha, blend and the exact clip
//! coverage: `out = mix(B, composite(B, S·alpha), cov)`. A default-blend, alpha-1 layer is then
//! exactly a clip (source-over is associative, 09 §3.1).

use peniko::BlendMode;

use super::compose::composite;

/// One open layer.
pub struct Layer {
    /// The layer's own content, premultiplied, over transparent.
    pub px: Vec<[f64; 4]>,
    /// The clip coverage of this layer (its own clip only; the parent applies its own).
    pub mask: Vec<f64>,
    pub blend: BlendMode,
    pub alpha: f64,
}

impl Layer {
    pub fn new(n: usize, mask: Vec<f64>, blend: BlendMode, alpha: f64) -> Self {
        Layer { px: vec![[0.0; 4]; n], mask, blend, alpha }
    }

    /// Composite this layer into `parent`.
    pub fn pop_into(self, parent: &mut [[f64; 4]]) {
        for ((b, s), &cov) in parent.iter_mut().zip(&self.px).zip(&self.mask) {
            if cov == 0.0 {
                continue;
            }
            let s = s.map(|v| v * self.alpha);
            let r = composite(*b, s, self.blend.mix, self.blend.compose);
            for k in 0..4 {
                b[k] += (r[k] - b[k]) * cov;
            }
        }
    }
}
