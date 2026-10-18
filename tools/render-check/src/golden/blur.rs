//! The blur kernel (09 §10.2, normative for L61/L62/R42 and this model) and the R42 blur
//! styles, on single-channel coverage. Every pass quantizes to 8 bits with round-to-nearest,
//! mirroring the renderer's `Rgba8Unorm` intermediates (V51 step 3).

/// Normalized 1-D Gaussian weights for `sigma` at the chosen level: `|i| ≤ ceil(3σ)`.
pub fn gaussian_weights(sigma: f64) -> Vec<f64> {
    let r = (3.0 * sigma).ceil().max(0.0);
    #[allow(clippy::cast_possible_truncation)] // radius of a small σ
    let r = r as i64;
    let w: Vec<f64> = (-r..=r).map(|i| (-(i * i) as f64 / (2.0 * sigma * sigma)).exp()).collect();
    let sum: f64 = w.iter().sum();
    w.into_iter().map(|v| v / sum).collect()
}

/// The downsample level `k` (smallest in `0..=3` with `σ / 2^k ≤ 2`) and `σk`.
pub fn level(sigma: f64) -> (u32, f64) {
    let mut k = 0;
    while k < 3 && sigma / f64::from(1u32 << k) > 2.0 {
        k += 1;
    }
    (k, sigma / f64::from(1u32 << k))
}

fn q8(v: f64) -> f64 {
    (v.clamp(0.0, 1.0) * 255.0).round() / 255.0
}

/// 2×2 box average (odd sizes clamp to edge), quantized.
fn downsample(src: &[f64], w: usize, h: usize) -> (Vec<f64>, usize, usize) {
    let (ow, oh) = (w.div_ceil(2).max(1), h.div_ceil(2).max(1));
    let at = |x: usize, y: usize| src[y.min(h - 1) * w + x.min(w - 1)];
    let out = (0..oh)
        .flat_map(|y| (0..ow).map(move |x| (x, y)))
        .map(|(x, y)| {
            q8((at(2 * x, 2 * y) + at(2 * x + 1, 2 * y) + at(2 * x, 2 * y + 1) + at(2 * x + 1, 2 * y + 1))
                / 4.0)
        })
        .collect();
    (out, ow, oh)
}

/// 2× bilinear upsample with aligned texel centers (clamp to edge), quantized, to `tw × th`.
fn upsample(src: &[f64], w: usize, h: usize, tw: usize, th: usize) -> Vec<f64> {
    let at = |x: i64, y: i64| {
        let x = usize::try_from(x.clamp(0, i64::try_from(w).unwrap_or(1) - 1)).unwrap_or(0);
        let y = usize::try_from(y.clamp(0, i64::try_from(h).unwrap_or(1) - 1)).unwrap_or(0);
        src[y * w + x]
    };
    (0..th)
        .flat_map(|y| (0..tw).map(move |x| (x, y)))
        .map(|(x, y)| {
            #[allow(clippy::cast_precision_loss)] // small sizes
            let (u, v) = ((x as f64 + 0.5) / 2.0 - 0.5, (y as f64 + 0.5) / 2.0 - 0.5);
            let (x0, y0) = (u.floor(), v.floor());
            let (fx, fy) = (u - x0, v - y0);
            #[allow(clippy::cast_possible_truncation)] // in range
            let (i, j) = (x0 as i64, y0 as i64);
            let top = at(i, j) * (1.0 - fx) + at(i + 1, j) * fx;
            let bot = at(i, j + 1) * (1.0 - fx) + at(i + 1, j + 1) * fx;
            q8(top * (1.0 - fy) + bot * fy)
        })
        .collect()
}

/// Blur coverage `src` (`w × h`, 0..1) by `sigma` device px; `decal` = transparent outside.
pub fn blur(src: &[f64], w: usize, h: usize, sigma: f64, decal: bool) -> Vec<f64> {
    if sigma <= 0.0 {
        return src.to_vec();
    }
    let (k, sk) = level(sigma);
    let mut levels = vec![(src.iter().map(|&v| q8(v)).collect::<Vec<f64>>(), w, h)];
    for _ in 0..k {
        let (buf, lw, lh) = levels.last().cloned().unwrap_or_default();
        levels.push(downsample(&buf, lw, lh));
    }
    let (mut buf, lw, lh) = levels.pop().unwrap_or_default();
    let weights = gaussian_weights(sk);
    let r = i64::try_from(weights.len() / 2).unwrap_or(0);
    let sample = |b: &[f64], x: i64, y: i64| -> f64 {
        let (wi, hi) = (i64::try_from(lw).unwrap_or(1), i64::try_from(lh).unwrap_or(1));
        if decal && (x < 0 || y < 0 || x >= wi || y >= hi) {
            return 0.0;
        }
        b[usize::try_from(y.clamp(0, hi - 1)).unwrap_or(0) * lw
            + usize::try_from(x.clamp(0, wi - 1)).unwrap_or(0)]
    };
    for horizontal in [true, false] {
        let prev = buf.clone();
        for y in 0..lh {
            for x in 0..lw {
                let (xi, yi) = (i64::try_from(x).unwrap_or(0), i64::try_from(y).unwrap_or(0));
                let v: f64 = weights
                    .iter()
                    .enumerate()
                    .map(|(n, wt)| {
                        let d = i64::try_from(n).unwrap_or(0) - r;
                        wt * if horizontal { sample(&prev, xi + d, yi) } else { sample(&prev, xi, yi + d) }
                    })
                    .sum();
                buf[y * lw + x] = q8(v);
            }
        }
    }
    // Upsample back through every level.
    let (mut cw, mut ch) = (lw, lh);
    while let Some((_, tw, th)) = levels.pop() {
        buf = upsample(&buf, cw, ch, tw, th);
        (cw, ch) = (tw, th);
    }
    buf
}

/// R42 blur styles on coverage `c` and blurred `b` (0..1, quantized like the renderer's 0..255).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlurStyle {
    Normal,
    Solid,
    Outer,
    Inner,
}

pub fn style(s: BlurStyle, c: f64, b: f64) -> f64 {
    let (c8, b8) = ((c * 255.0).round(), (b * 255.0).round());
    let v = match s {
        BlurStyle::Normal => b8,
        BlurStyle::Solid => c8.max(b8),
        BlurStyle::Outer => (b8 * (255.0 - c8) / 255.0).round(),
        BlurStyle::Inner => (b8 * c8 / 255.0).round(),
    };
    v / 255.0
}
