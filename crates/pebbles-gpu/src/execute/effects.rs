//! Effect layers on the GPU (09 §10.2–§10.6, L61/L63/L64): after an effect layer's last pass,
//! its texture runs through the steps' passes (pool textures, Rgba8Unorm, one full-target
//! triangle each); the layer composites from the final texture.

use super::pipelines::{PipelineKey, PipelineKind};
use crate::gpu_types::EffectParams;
use crate::prepare::plan::EffectStep;
use crate::stats::FrameStats;

/// Largest Gaussian radius one pass holds (`EffectParams::weights`: 28 taps).
const MAX_BLUR_RADIUS: usize = 27;
/// Morphology window radius per pass; larger radii repeat passes (max of maxes is exact).
const MAX_MORPH_RADIUS: u32 = 32;

/// One effect pass: `src` pool slot → `dst` pool slot (`dst_size` content px).
#[derive(Clone, Debug)]
pub(crate) struct EffectPass {
    pub kind: PipelineKind,
    pub variant: u8,
    pub src: usize,
    pub dst: usize,
    pub dst_size: (u32, u32),
    pub params: EffectParams,
    /// Index of this pass's params slot.
    pub slot: u32,
}

fn params(src: (u32, u32), dst: (u32, u32)) -> EffectParams {
    let mut p: EffectParams = bytemuck::Zeroable::zeroed();
    p.size = [src.0 as f32, src.1 as f32, dst.0 as f32, dst.1 as f32];
    p
}

/// Expand `steps` over a `size` content rect starting from `src`; `acquire(size)` returns a
/// fresh pool slot. Returns the final slot (`src` when every step is an identity).
pub(crate) fn plan_passes(
    steps: &[EffectStep],
    size: (u32, u32),
    src: usize,
    acquire: &mut dyn FnMut((u32, u32)) -> usize,
    next_slot: &mut u32,
    out: &mut Vec<EffectPass>,
) -> usize {
    let mut cur = (src, size);
    let mut push = |kind,
                    variant,
                    from: (usize, (u32, u32)),
                    to: (u32, u32),
                    mut p: EffectParams,
                    out: &mut Vec<EffectPass>| {
        let dst = acquire(to);
        p.size = [from.1.0 as f32, from.1.1 as f32, to.0 as f32, to.1 as f32];
        out.push(EffectPass { kind, variant, src: from.0, dst, dst_size: to, params: p, slot: *next_slot });
        *next_slot += 1;
        (dst, to)
    };
    for step in steps {
        match *step {
            EffectStep::Blur { sigma_x, sigma_y, clamp } => {
                if sigma_x <= 0.0 && sigma_y <= 0.0 {
                    continue;
                }
                let (k, sx, sy) = crate::prepare::effects::blur_level(sigma_x, sigma_y);
                let mut sizes = vec![cur.1];
                for _ in 0..k {
                    let (w, h) = *sizes.last().unwrap_or(&cur.1);
                    let next = (w.div_ceil(2).max(1), h.div_ceil(2).max(1));
                    cur = push(PipelineKind::Blur, 0, cur, next, params(cur.1, next), out);
                    sizes.push(next);
                }
                for (axis, s) in [(0u32, sx), (1, sy)] {
                    if s <= 0.0 {
                        continue;
                    }
                    let w = crate::raster::blur::weights(s);
                    let r = (w.len() / 2).min(MAX_BLUR_RADIUS);
                    let mut p = params(cur.1, cur.1);
                    p.info = [0, u32::try_from(r).unwrap_or(0), u32::from(clamp), axis];
                    let sum: f32 = w[w.len() / 2 - r..=w.len() / 2 + r].iter().sum();
                    for (i, v) in w[w.len() / 2..=w.len() / 2 + r].iter().enumerate() {
                        p.weights[i / 4][i % 4] = v / sum;
                    }
                    let to = cur.1;
                    cur = push(PipelineKind::Blur, 1, cur, to, p, out);
                }
                for i in (0..k as usize).rev() {
                    let to = sizes[i];
                    cur = push(PipelineKind::Blur, 2, cur, to, params(cur.1, to), out);
                }
            }
            EffectStep::ColorMatrix(m) => {
                let mut p = params(cur.1, cur.1);
                for (i, v) in m.iter().enumerate() {
                    p.matrix[i / 4][i % 4] = *v;
                }
                let to = cur.1;
                cur = push(PipelineKind::ColorMatrix, 0, cur, to, p, out);
            }
            EffectStep::Dilate { rx, ry } | EffectStep::Erode { rx, ry } => {
                let variant = u8::from(matches!(step, EffectStep::Erode { .. }));
                for (axis, mut r) in [(0u32, rx), (1, ry)] {
                    while r > 0 {
                        let n = r.min(MAX_MORPH_RADIUS);
                        let mut p = params(cur.1, cur.1);
                        p.info = [0, n, 0, axis];
                        let to = cur.1;
                        cur = push(PipelineKind::Morphology, variant, cur, to, p, out);
                        r -= n;
                    }
                }
            }
        }
    }
    cur.0
}

/// Ensure the pipelines `passes` use.
pub(crate) fn ensure_pipelines(
    renderer: &mut crate::renderer::Renderer,
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    passes: &[EffectPass],
    stats: &mut FrameStats,
) {
    for p in passes {
        let key =
            PipelineKey { kind: p.kind, color_format: format, sample_count: 1, blend: 0, variant: p.variant };
        renderer.pipelines.ensure(device, &renderer.shaders, &renderer.layouts, key, stats);
    }
}
