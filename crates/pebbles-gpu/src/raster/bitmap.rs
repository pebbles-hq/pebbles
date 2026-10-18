//! Bitmap-strike color glyphs (02 §7.2 "Bitmap strikes", R23): decoded at the strike's own
//! ppem, premultiplied, then resized here — never by swash, whose Mitchell filter runs on
//! straight alpha and darkens edges.

/// Box-filter `src` (`w`×`h` premultiplied RGBA8) by `s` (`0 < s ≤ 1`) into `out`: output size
/// `(ceil(w·s), ceil(h·s))`; each output pixel is the area-weighted mean of the source texels
/// under `[x/s, (x+1)/s) × [y/s, (y+1)/s)` clipped to the source, rounded to nearest.
pub(crate) fn downscale_premul(src: &[u8], w: u32, h: u32, s: f32, out: &mut Vec<u8>) -> (u32, u32) {
    let s = f64::from(s);
    // `- 1e-4`: f32 scales like 1/3 widen by an ulp in f64 (3 · ⅓ = 1.00000003).
    let dims = |n: u32| crate::prepare::num::ceil_px(f64::from(n) * s - 1e-4, n.max(1)).unwrap_or(1).max(1);
    let (ow, oh) = (dims(w), dims(h));
    out.clear();
    out.resize(ow as usize * oh as usize * 4, 0);
    // Per output index: the source span `[i/s, (i+1)/s)` clipped to `[0, n)`.
    let span = |i: u32, n: u32| (f64::from(i) / s, (f64::from(i + 1) / s).min(f64::from(n)));
    for oy in 0..oh {
        let (y0, y1) = span(oy, h);
        for ox in 0..ow {
            let (x0, x1) = span(ox, w);
            let mut acc = [0.0f64; 4];
            let mut area = 0.0;
            let mut sy = y0.floor();
            while sy < y1 {
                let wy = (sy + 1.0).min(y1) - sy.max(y0);
                let mut sx = x0.floor();
                while sx < x1 {
                    let wx = (sx + 1.0).min(x1) - sx.max(x0);
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // in-bounds texel
                    let i = ((sy as usize) * w as usize + sx as usize) * 4;
                    if let Some(px) = src.get(i..i + 4) {
                        for (a, &c) in acc.iter_mut().zip(px) {
                            *a += f64::from(c) * wx * wy;
                        }
                    }
                    area += wx * wy;
                    sx += 1.0;
                }
                sy += 1.0;
            }
            let o = (oy as usize * ow as usize + ox as usize) * 4;
            for (d, a) in out[o..o + 4].iter_mut().zip(acc) {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // 0..=255
                {
                    *d = if area > 0.0 { (a / area).round().clamp(0.0, 255.0) as u8 } else { 0 };
                }
            }
        }
    }
    (ow, oh)
}

/// The device quad of a strike bitmap drawn `s` × its size (`s > 1`: the strike is smaller than
/// requested and is uploaded unchanged): `(x0, y0, x1, y1)` for pen `(ix, iy)`.
pub(crate) fn scaled_strike_quad(ix: f32, iy: f32, left: i32, top: i32, w: u32, h: u32, s: f32) -> [f32; 4] {
    #[allow(clippy::cast_precision_loss)] // bitmap metrics ≤ 2^16
    let (left, top, w, h) = (left as f32, top as f32, w as f32, h as f32);
    let x0 = ix + (left * s).round();
    let y0 = iy - (top * s).round();
    [x0, y0, x0 + (w * s).round(), y0 + (h * s).round()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downscale_has_no_dark_fringe() {
        // Straight [255,0,0,255] ×3 columns + transparent black; premultiplied it is unchanged.
        let row: Vec<u8> = [[255, 0, 0, 255], [255, 0, 0, 255], [255, 0, 0, 255], [0, 0, 0, 0]].concat();
        let src = row.repeat(4);
        let mut out = Vec::new();
        assert_eq!(downscale_premul(&src, 4, 4, 0.5, &mut out), (2, 2));
        assert_eq!(&out[4..8], &[128, 0, 0, 128], "premultiplied mean, R == A");
    }

    #[test]
    fn downscale_box_is_exact() {
        let src: Vec<u8> = (0..9).flat_map(|i| if i % 2 == 0 { [255u8; 4] } else { [0u8; 4] }).collect();
        let mut out = Vec::new();
        assert_eq!(downscale_premul(&src, 3, 3, 1.0 / 3.0, &mut out), (1, 1));
        // 5 white of 9 → 255·5/9 = 141.67.
        assert_eq!(out, [142; 4]);
    }

    #[test]
    fn small_strike_quad_is_scaled() {
        // A 136-px strike requested at ppem 272: s = 2, the quad is twice the bitmap.
        let s = 272.0 / 136.0;
        let [x0, y0, x1, y1] = scaled_strike_quad(10.0, 300.0, 4, 120, 136, 128, s);
        assert_eq!((x1 - x0, y1 - y0), (272.0, 256.0));
        assert_eq!((x0, y0), (18.0, 60.0));
    }
}
