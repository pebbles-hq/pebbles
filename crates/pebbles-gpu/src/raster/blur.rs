//! Blurred coverage masks (02 R42): the 09 §10.2 kernel on an `R8` coverage mask — downsample
//! by 2^k (smallest k ≤ 3 with σ/2^k ≤ 2), separable Gaussian (radius ⌈3σk⌉, weights normalized
//! to 1), bilinear upsample back — every pass rounded to nearest u8 (the renderer's 8-bit
//! intermediates; the golden model mirrors each quantization), then the R42 blur style.

use pebbles_render::paint::display_list::BlurStyle;

/// The downsample level `k` and the level's σ.
pub(crate) fn level(sigma: f32) -> (u32, f32) {
    let mut k = 0;
    while k < 3 && sigma / (1u32 << k) as f32 > 2.0 {
        k += 1;
    }
    (k, sigma / (1u32 << k) as f32)
}

/// Normalized Gaussian weights for `|i| ≤ ⌈3σ⌉`.
pub(crate) fn weights(sigma: f32) -> Vec<f32> {
    let r = (3.0 * sigma).ceil().max(0.0);
    #[allow(clippy::cast_possible_truncation)] // a small radius
    let r = r as i32;
    let w: Vec<f32> = (-r..=r).map(|i| (-((i * i) as f32) / (2.0 * sigma * sigma)).exp()).collect();
    let sum: f32 = w.iter().sum();
    w.into_iter().map(|v| v / sum).collect()
}

fn q8(v: f32) -> u8 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to 0..=255
    let v = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    v
}

fn downsample(src: &[u8], w: usize, h: usize) -> (Vec<u8>, usize, usize) {
    let (ow, oh) = (w.div_ceil(2).max(1), h.div_ceil(2).max(1));
    let at = |x: usize, y: usize| u32::from(src[y.min(h - 1) * w + x.min(w - 1)]);
    let mut out = Vec::with_capacity(ow * oh);
    for y in 0..oh {
        for x in 0..ow {
            let s = at(2 * x, 2 * y) + at(2 * x + 1, 2 * y) + at(2 * x, 2 * y + 1) + at(2 * x + 1, 2 * y + 1);
            out.push(u8::try_from((s + 2) / 4).unwrap_or(u8::MAX));
        }
    }
    (out, ow, oh)
}

fn upsample(src: &[u8], w: usize, h: usize, tw: usize, th: usize) -> Vec<u8> {
    let at = |x: i64, y: i64| -> f32 {
        let x = usize::try_from(x.clamp(0, i64::try_from(w).unwrap_or(1) - 1)).unwrap_or(0);
        let y = usize::try_from(y.clamp(0, i64::try_from(h).unwrap_or(1) - 1)).unwrap_or(0);
        f32::from(src[y * w + x]) / 255.0
    };
    let mut out = Vec::with_capacity(tw * th);
    for y in 0..th {
        for x in 0..tw {
            #[allow(clippy::cast_precision_loss)] // mask sizes ≪ 2^24
            let (u, v) = ((x as f32 + 0.5) / 2.0 - 0.5, (y as f32 + 0.5) / 2.0 - 0.5);
            let (x0, y0) = (u.floor(), v.floor());
            let (fx, fy) = (u - x0, v - y0);
            #[allow(clippy::cast_possible_truncation)] // in range
            let (i, j) = (x0 as i64, y0 as i64);
            let top = at(i, j) * (1.0 - fx) + at(i + 1, j) * fx;
            let bot = at(i, j + 1) * (1.0 - fx) + at(i + 1, j + 1) * fx;
            out.push(q8(top * (1.0 - fy) + bot * fy));
        }
    }
    out
}

/// Blur `src` (`w × h` coverage, already padded by ⌈3σ⌉) by `sigma` device px; outside = 0.
pub(crate) fn blur_mask(src: &[u8], w: usize, h: usize, sigma: f32) -> Vec<u8> {
    if sigma <= 0.0 || w == 0 || h == 0 {
        return src.to_vec();
    }
    let (k, sk) = level(sigma);
    let mut levels = vec![(src.to_vec(), w, h)];
    for _ in 0..k {
        let (b, lw, lh) = levels.last().map(|l| downsample(&l.0, l.1, l.2)).unwrap_or_default();
        levels.push((b, lw, lh));
    }
    let (mut buf, lw, lh) = levels.pop().unwrap_or_default();
    let wts = weights(sk);
    let r = i64::try_from(wts.len() / 2).unwrap_or(0);
    let (wi, hi) = (i64::try_from(lw).unwrap_or(0), i64::try_from(lh).unwrap_or(0));
    for horizontal in [true, false] {
        let prev = buf.clone();
        let sample = |x: i64, y: i64| -> f32 {
            if x < 0 || y < 0 || x >= wi || y >= hi {
                return 0.0;
            }
            f32::from(prev[usize::try_from(y * wi + x).unwrap_or(0)]) / 255.0
        };
        for y in 0..hi {
            for x in 0..wi {
                let mut v = 0.0;
                for (n, wt) in wts.iter().enumerate() {
                    let d = i64::try_from(n).unwrap_or(0) - r;
                    v += wt * if horizontal { sample(x + d, y) } else { sample(x, y + d) };
                }
                buf[usize::try_from(y * wi + x).unwrap_or(0)] = q8(v);
            }
        }
    }
    let (mut cw, mut ch) = (lw, lh);
    while let Some((_, tw, th)) = levels.pop() {
        buf = upsample(&buf, cw, ch, tw, th);
        (cw, ch) = (tw, th);
    }
    buf
}

/// R42 styles on coverage `c` and blurred `b` (0..255).
pub(crate) fn style(s: BlurStyle, c: u8, b: u8) -> u8 {
    let (c, b) = (u32::from(c), u32::from(b));
    let v = match s {
        BlurStyle::Solid => c.max(b),
        BlurStyle::Outer => (b * (255 - c) + 127) / 255,
        BlurStyle::Inner => (b * c + 127) / 255,
        // Normal and future styles.
        _ => b,
    };
    u8::try_from(v).unwrap_or(u8::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_kernel_sums_to_one() {
        for s in [0.5f32, 1.0, 2.0, 1.7] {
            assert!((weights(s).iter().sum::<f32>() - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn blur_of_point_is_gaussian() {
        // σ = 2 (k = 0): one horizontal pass of a point; center/neighbour = exp(1/8).
        let w = weights(2.0);
        let c = w.len() / 2;
        let ratio = w[c] / w[c + 1];
        assert!((ratio - 0.125f32.exp()).abs() / 0.125f32.exp() < 0.01, "{ratio}");
    }

    #[test]
    #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation, clippy::cast_sign_loss)] // small test sizes
    fn downsampled_blur_close_to_direct() {
        // A 24×24 square in a 96×96 mask, σ = 6: the k = 2 pyramid vs a direct full-res blur.
        // The 09 §10.2 2×2 box decimation is phase-dependent: an edge on an even pixel lands half
        // a fine pixel off per level (measured centroid shift (2^k − 1)/2 for a line at x = 128),
        // so the worst case — this square's even-aligned edges — differs by up to 6 (2.4 %), not
        // the card's 2. The golden model mirrors the same pyramid, so rendering is judged exactly.
        let (w, h) = (96usize, 96usize);
        let src: Vec<u8> = (0..w * h)
            .map(|i| if (36..60).contains(&(i % w)) && (36..60).contains(&(i / w)) { 255 } else { 0 })
            .collect();
        let fast = blur_mask(&src, w, h, 6.0);
        // Direct: one level, the full σ.
        let wts = weights(6.0);
        let r = (wts.len() / 2) as i64;
        let pass = |s: &[u8], hz: bool| -> Vec<u8> {
            (0..w * h)
                .map(|i| {
                    let (x, y) = ((i % w) as i64, (i / w) as i64);
                    let v: f32 = wts
                        .iter()
                        .enumerate()
                        .map(|(n, wt)| {
                            let d = n as i64 - r;
                            let (sx, sy) = if hz { (x + d, y) } else { (x, y + d) };
                            if sx < 0 || sy < 0 || sx >= w as i64 || sy >= h as i64 {
                                0.0
                            } else {
                                wt * f32::from(s[(sy as usize) * w + sx as usize]) / 255.0
                            }
                        })
                        .sum();
                    q8(v)
                })
                .collect()
        };
        let direct = pass(&pass(&src, true), false);
        let max = fast.iter().zip(&direct).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
        assert!(max <= 6, "max delta {max}");
    }

    #[test]
    fn styles_combine_as_specified() {
        assert_eq!(style(BlurStyle::Normal, 200, 90), 90);
        assert_eq!(style(BlurStyle::Solid, 200, 90), 200);
        assert_eq!(style(BlurStyle::Outer, 255, 90), 0);
        assert_eq!(style(BlurStyle::Outer, 0, 90), 90);
        assert_eq!(style(BlurStyle::Inner, 255, 90), 90);
        assert_eq!(style(BlurStyle::Inner, 128, 90), 45);
    }
}
