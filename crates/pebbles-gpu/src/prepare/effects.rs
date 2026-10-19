//! Effect layers (09 §10, L61–L64): a `LayerEffect` in device space — blur σ, morphology radii
//! and nothing else scale by `s`, the layer transform's larger singular value; chains flatten.

use pebbles_render::paint::display_list::{BlurEdge, LayerEffect};

use super::plan::EffectStep;

/// Append `effect`'s steps (device px) to `out`.
pub(crate) fn device_steps(effect: &LayerEffect, s: f64, out: &mut Vec<EffectStep>) {
    let sc = |v: f32| super::num::narrow_f32(f64::from(v.max(0.0)) * s);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // rounded, clamped ≥ 0, ≤ 4096
    let px = |v: f32| (f64::from(v.max(0.0)) * s).round().min(4096.0) as u32;
    match effect {
        LayerEffect::Blur { sigma_x, sigma_y, edge } => {
            out.push(EffectStep::Blur {
                sigma_x: sc(*sigma_x),
                sigma_y: sc(*sigma_y),
                clamp: *edge == BlurEdge::Clamp,
            });
        }
        LayerEffect::ColorMatrix(m) => out.push(EffectStep::ColorMatrix(*m)),
        LayerEffect::Dilate { radius_x, radius_y } => {
            out.push(EffectStep::Dilate { rx: px(*radius_x), ry: px(*radius_y) })
        }
        LayerEffect::Erode { radius_x, radius_y } => {
            out.push(EffectStep::Erode { rx: px(*radius_x), ry: px(*radius_y) })
        }
        LayerEffect::Chain(es) => {
            for e in es.iter() {
                device_steps(e, s, out);
            }
        }
        _ => {}
    }
}

/// The 09 §10.2 blur level: downsample `k` (`0..=3`, smallest with σ/2^k ≤ 2) and each axis' σ
/// at that level. Both axes share `k` (the 2×2 box downsample).
pub(crate) fn blur_level(sigma_x: f32, sigma_y: f32) -> (u32, f32, f32) {
    let (k, _) = crate::raster::blur::level(sigma_x.max(sigma_y));
    let d = (1u32 << k) as f32;
    (k, sigma_x / d, sigma_y / d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn blur_steps_follow_spec() {
        let (k, sx, _) = blur_level(7.0, 7.0);
        assert_eq!((k, sx), (2, 1.75));
        assert_eq!(crate::raster::blur::weights(sx).len(), 2 * 6 + 1, "r = ceil(3 · 1.75) = 6");
    }

    #[test]
    fn chain_flattens_and_sums_reach() {
        let blur = LayerEffect::Blur { sigma_x: 2.0, sigma_y: 1.0, edge: BlurEdge::Decal };
        let inner = LayerEffect::chain([blur.clone(), LayerEffect::Dilate { radius_x: 3.0, radius_y: 4.0 }]);
        let chain = LayerEffect::chain([inner, LayerEffect::ColorMatrix([0.0; 20])]);
        let LayerEffect::Chain(es) = &chain else { panic!() };
        assert_eq!(es.len(), 3, "nested chain flattened");
        assert_eq!(chain.reach(), (6.0 + 3.0, 3.0 + 4.0));
        let mut steps = Vec::new();
        device_steps(&LayerEffect::Chain(Arc::clone(es)), 2.0, &mut steps);
        assert_eq!(steps[0], EffectStep::Blur { sigma_x: 4.0, sigma_y: 2.0, clamp: false });
        assert_eq!(steps[1], EffectStep::Dilate { rx: 6, ry: 8 });
    }
}
