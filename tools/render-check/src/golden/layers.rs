//! Clips and layers (09 §2–§3, V45): every `PushLayer` is an isolated `f64` buffer over
//! transparent, composited into its parent on pop with group alpha, blend and the exact clip
//! coverage: `out = mix(B, composite(B, S·alpha), cov)`. A default-blend, alpha-1 layer is then
//! exactly a clip (source-over is associative, 09 §3.1).

use peniko::BlendMode;

use super::compose::composite;

/// An effect layer's device-space steps and the device rect `(x0, y0, x1, y1)` they run over.
pub type EffectSpec = (Vec<super::effects::Step>, (usize, usize, usize, usize));

/// One open layer.
pub struct Layer {
    /// The layer's own content, premultiplied, over transparent.
    pub px: Vec<[f64; 4]>,
    /// The clip coverage of this layer (its own clip only; the parent applies its own).
    pub mask: Vec<f64>,
    pub blend: BlendMode,
    pub alpha: f64,
    /// An effect layer (09 §10.3): its device-space steps and the device rect they run over.
    pub effect: Option<EffectSpec>,
}

impl Layer {
    pub fn new(n: usize, mask: Vec<f64>, blend: BlendMode, alpha: f64) -> Self {
        Layer { px: vec![[0.0; 4]; n], mask, blend, alpha, effect: None }
    }

    /// A default-blend, alpha-1 layer without an effect: exactly a clip (drawn inline on the
    /// GPU, 09 §3.1).
    pub fn is_pure_clip(&self) -> bool {
        self.effect.is_none() && self.alpha >= 1.0 && self.blend == BlendMode::default()
    }

    /// Apply the effect (if any) to the layer's 8-bit content over its rect (`w` = target width).
    pub fn run_effect(&mut self, w: usize) {
        let Some((steps, r)) = self.effect.take() else { return };
        super::effects::quantize(&mut self.px);
        let img = super::effects::crop(&self.px, w, r);
        let out = super::effects::apply(img, &steps, false);
        let mut px = vec![[0.0; 4]; self.px.len()];
        for y in 0..out.h {
            let row = (r.1 + y) * w + r.0;
            px[row..row + out.w].copy_from_slice(&out.px[y * out.w..(y + 1) * out.w]);
        }
        self.px = px;
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
