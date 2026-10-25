//! The precomputed rounded-corner blur profile for box shadows (M33; 02 §8.2, R40), the
//! industry approach (Skia's blurred-rrect profile textures): instead of integrating a corner
//! per pixel, `shadow.wgsl` reads a table.
//!
//! In σ units, with the corner arc's center at the origin, a quarter disc of radius ρ = r/σ has
//! rows `t ∈ [0, ρ]` of half width `w(t) = √(ρ² − t²)`. The table holds
//!
//! ```text
//! T(u, v; ρ) = ∫₀^ρ φ(v − t) · Φ(w(t) − u) dt
//! ```
//!
//! (φ, Φ: the standard normal density and CDF), the Gaussian-weighted mass of the corner band's
//! rows reaching past `u`. `prepare/shadows.rs` shows how the shadow value is assembled from
//! it; the shader reads it with `textureLoad` and interpolates by hand (bilinear in `(u, v)`,
//! linear across the two nearest ρ levels), like every other pebbles lookup, so the result does
//! not depend on the backend's filtering.
//!
//! Levels: ρ = 0, `RHO_STEP`, …, `RHO_MAX`; each is a `CELL`×`CELL` grid with step `STEP` over
//! `[−PAD, ρ + PAD]²` (clamped at the edges, where `T` is constant to 3·10⁻⁵). Levels sit in a
//! `COLS`-wide grid of cells of an `R32Float` texture (`textureLoad` needs no filterable format,
//! so it works on WebGL2/GLES 3.0). Corners with ρ > `RHO_MAX` keep the per-pixel integration.

use std::sync::OnceLock;

/// Table step in σ.
pub(crate) const STEP: f32 = 0.125;
/// ρ step between levels.
pub(crate) const RHO_STEP: f32 = 0.125;
/// The largest tabulated ρ (mirrored in `shadow.wgsl`; read by the tests).
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const RHO_MAX: f32 = 4.0;
/// The table covers `[−PAD, ρ + PAD]` in both axes.
pub(crate) const PAD: f32 = 4.0;
/// Levels (ρ = 0 … `RHO_MAX`).
pub(crate) const LEVELS: u32 = 33;
/// Cell side in texels (fits the largest level: `(RHO_MAX + 2·PAD)/STEP + 1`).
pub(crate) const CELL: u32 = 97;
/// Cells per texture row.
pub(crate) const COLS: u32 = 16;
/// Texture size.
pub(crate) const WIDTH: u32 = CELL * COLS;
pub(crate) const HEIGHT: u32 = CELL * LEVELS.div_ceil(COLS);
// WebGL2 guarantees 2048 texels per side.
const _: () = assert!(WIDTH <= 2048 && HEIGHT <= 2048);

/// Θ nodes of the composite Simpson rule used to build the table (even).
const NODES: usize = 256;

/// Side of level 0: `2·PAD/STEP + 1`.
const LEVEL0_SIZE: u32 = 65;

/// Texels per side of level `i`.
pub(crate) fn level_size(i: u32) -> u32 {
    // ρ_i = i·RHO_STEP; side = (ρ_i + 2·PAD)/STEP + 1, and RHO_STEP = STEP.
    i + LEVEL0_SIZE
}

/// erf after Abramowitz & Stegun 7.1.26 in f64 (the shader's formula).
fn erf(x: f64) -> f64 {
    let ax = x.abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * ax);
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736 + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    (1.0 - poly * (-ax * ax).exp()).copysign(x)
}

fn phi_cdf(z: f64) -> f64 {
    0.5 * (1.0 + erf(z * std::f64::consts::FRAC_1_SQRT_2))
}

/// The texels, row-major `WIDTH`×`HEIGHT`; built once per process.
pub(crate) fn texels() -> &'static [f32] {
    static TABLE: OnceLock<Vec<f32>> = OnceLock::new();
    TABLE.get_or_init(build)
}

fn build() -> Vec<f32> {
    let mut out = vec![0f32; (WIDTH * HEIGHT) as usize];
    let (step, pad) = (f64::from(STEP), f64::from(PAD));
    for level in 0..LEVELS {
        let rho = f64::from(level) * f64::from(RHO_STEP);
        let n = level_size(level) as usize;
        let (cx, cy) = ((level % COLS * CELL) as usize, (level / COLS * CELL) as usize);
        if rho == 0.0 {
            continue;
        }
        // Simpson over θ: t = ρ·sin θ, w = ρ·cos θ, dt = ρ·cos θ dθ (smooth at the arc's top).
        let h = std::f64::consts::FRAC_PI_2 / NODES as f64;
        let nodes: Vec<(f64, f64, f64)> = (0..=NODES)
            .map(|k| {
                let th = k as f64 * h;
                let simpson = if k == 0 || k == NODES {
                    1.0
                } else if k % 2 == 1 {
                    4.0
                } else {
                    2.0
                };
                (rho * th.sin(), rho * th.cos(), simpson * h / 3.0 * rho * th.cos())
            })
            .collect();
        let grid = |j: usize| -pad + j as f64 * step;
        // Separable: weights per (v, node) and CDFs per (u, node).
        let inv_sqrt_2pi = 1.0 / (2.0 * std::f64::consts::PI).sqrt();
        let wv: Vec<f64> = (0..n)
            .flat_map(|j| {
                let v = grid(j);
                nodes.iter().map(move |&(t, _, wt)| wt * inv_sqrt_2pi * (-0.5 * (v - t) * (v - t)).exp())
            })
            .collect();
        let cu: Vec<f64> =
            (0..n).flat_map(|i| nodes.iter().map(move |&(_, w, _)| phi_cdf(w - grid(i)))).collect();
        let m = nodes.len();
        for j in 0..n {
            let wrow = &wv[j * m..(j + 1) * m];
            for i in 0..n {
                let crow = &cu[i * m..(i + 1) * m];
                let t: f64 = wrow.iter().zip(crow).map(|(a, b)| a * b).sum();
                out[(cy + j) * WIDTH as usize + cx + i] = crate::prepare::num::narrow_f32(t);
            }
        }
    }
    out
}

/// The GPU side: the texture and its group-1 bind group for the shadow pipeline.
pub(crate) struct ShadowProfile {
    _texture: wgpu::Texture,
    pub bind_group: wgpu::BindGroup,
}

impl ShadowProfile {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue, layout: &wgpu::BindGroupLayout) -> Self {
        let size = wgpu::Extent3d { width: WIDTH, height: HEIGHT, depth_or_array_layers: 1 };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pebbles.gpu.texture.shadow_profile"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            texture.as_image_copy(),
            bytemuck::cast_slice(texels()),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(WIDTH * 4),
                rows_per_image: Some(HEIGHT),
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pebbles.gpu.bind.shadow_profile"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            }],
        });
        Self { _texture: texture, bind_group }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_fits() {
        assert_eq!(level_size(0) as f32, 2.0 * PAD / STEP + 1.0);
        assert_eq!(RHO_STEP, STEP);
        assert_eq!(level_size(LEVELS - 1), CELL);
        assert_eq!(f32::from(u16::try_from(LEVELS - 1).unwrap()) * RHO_STEP, RHO_MAX);
    }

    #[test]
    fn edges_are_constant() {
        // u ≪ 0: the band's full mass Φ(v) − Φ(v − ρ); far right / far above: 0.
        let t = texels();
        let level = LEVELS - 1;
        let n = level_size(level) as usize;
        let (cx, cy) = ((level % COLS * CELL) as usize, (level / COLS * CELL) as usize);
        let at = |i: usize, j: usize| t[(cy + j) * WIDTH as usize + cx + i];
        let rho = 4.0f64;
        for j in [0, n / 3, n / 2, n - 1] {
            let v = -4.0 + j as f64 * 0.125;
            let mass = phi_cdf(v) - phi_cdf(v - rho);
            assert!((f64::from(at(0, j)) - mass).abs() < 1e-4, "{j}");
            assert!(at(n - 1, j).abs() < 1e-4);
        }
    }
}
