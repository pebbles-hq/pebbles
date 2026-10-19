//! Mipmapped minification (02 R35): a CPU box-filter mip chain built once at upload, and the
//! LOD computed once per draw on the CPU so every backend samples the same level (no
//! derivative-based hardware LOD).

/// Levels 1.. of premultiplied RGBA8 `level0` (`w × h`): each halves each side
/// (`max(1, n / 2)`); a texel is `(a + b + c + d + 2) >> 2` per channel of its 2×2 block
/// (odd sizes clamp to edge). Returns `(pixels, width, height)` per level.
pub(crate) fn build_mips(level0: &[u8], w: u32, h: u32) -> Vec<(Vec<u8>, u32, u32)> {
    let mut out: Vec<(Vec<u8>, u32, u32)> = Vec::new();
    let (mut cw, mut ch) = (w, h);
    while cw > 1 || ch > 1 {
        let (nw, nh) = ((cw / 2).max(1), (ch / 2).max(1));
        let src: &[u8] = out.last().map_or(level0, |l| l.0.as_slice());
        let at = |x: u32, y: u32, c: usize| -> u32 {
            let i = ((y.min(ch - 1) * cw + x.min(cw - 1)) * 4) as usize + c;
            u32::from(src.get(i).copied().unwrap_or(0))
        };
        let mut level = Vec::with_capacity((nw * nh * 4) as usize);
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let s = at(2 * x, 2 * y, c)
                        + at(2 * x + 1, 2 * y, c)
                        + at(2 * x, 2 * y + 1, c)
                        + at(2 * x + 1, 2 * y + 1, c);
                    level.push(u8::try_from((s + 2) >> 2).unwrap_or(u8::MAX));
                }
            }
        }
        out.push((level, nw, nh));
        (cw, ch) = (nw, nh);
    }
    out
}

/// `floor(log2(max(w, h))) + 1`.
pub(crate) fn mip_level_count(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

/// The LOD of an image drawn under the 2×2 linear part `[a, b, c, d]` (image px → device px):
/// `ρ = 1 / σ_min`, `lod = max(0, log2 ρ)`.
pub(crate) fn lod(a: f64, b: f64, c: f64, d: f64) -> f64 {
    let s1 = a * a + b * b + c * c + d * d;
    let det = (a * d - b * c).abs();
    let disc = (s1 * s1 - 4.0 * det * det).max(0.0).sqrt();
    let smin = ((s1 - disc) / 2.0).max(0.0).sqrt();
    if smin > 0.0 { (1.0 / smin).log2().max(0.0) } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mip_chain_sizes() {
        let levels = build_mips(&vec![0u8; 1000 * 300 * 4], 1000, 300);
        assert_eq!(levels.len() + 1, 10);
        assert_eq!(mip_level_count(1000, 300), 10);
        assert_eq!(levels.last().map(|l| (l.1, l.2)), Some((1, 1)));
        assert_eq!((levels[0].1, levels[0].2), (500, 150));
    }

    #[test]
    fn mip_box_average_exact() {
        // 2×2 → 1×1: (10 + 20 + 30 + 41 + 2) >> 2 = 25.
        let px: Vec<u8> = [10u8, 20, 30, 41].iter().flat_map(|&v| [v, v, v, 255]).collect();
        let levels = build_mips(&px, 2, 2);
        assert_eq!(levels[0].0, vec![25, 25, 25, 255]);
        // Odd width clamps to edge: 3×1 → 1×1 averages texels 0, 1 (twice each row-clamped).
        let px: Vec<u8> = [0u8, 100, 200].iter().flat_map(|&v| [v, 0, 0, 255]).collect();
        let levels = build_mips(&px, 3, 1);
        assert_eq!(levels[0].0[0], 50, "(0 + 100 + 0 + 100 + 2) >> 2");
    }

    #[test]
    fn lod_is_transform_based() {
        assert!((lod(0.25, 0.0, 0.0, 0.25) - 2.0).abs() < 1e-6);
        let (s, c) = (std::f64::consts::FRAC_PI_4.sin() * 0.5, std::f64::consts::FRAC_PI_4.cos() * 0.5);
        assert!((lod(c, s, -s, c) - 1.0).abs() < 1e-6);
        assert_eq!(lod(3.0, 0.0, 0.0, 3.0), 0.0);
    }
}
