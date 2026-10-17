//! Image type and the diff metric (§6.10) — hand-rolled, no image crates (DR-09).

use std::fmt;

/// Tightly packed RGBA8 (`width * height * 4` bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba8 {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Rgba8 {
    /// A `width × height` image filled with `px`.
    pub fn filled(width: u32, height: u32, px: [u8; 4]) -> Self {
        Self { width, height, data: px.repeat(width as usize * height as usize) }
    }

    /// The pixel at `(x, y)`.
    pub fn px(&self, x: u32, y: u32) -> [u8; 4] {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        [self.data[i], self.data[i + 1], self.data[i + 2], self.data[i + 3]]
    }

    /// Overwrite the pixel at `(x, y)`.
    pub fn set(&mut self, x: u32, y: u32, p: [u8; 4]) {
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.data[i..i + 4].copy_from_slice(&p);
    }

    /// Number of pixels.
    pub fn pixels(&self) -> usize {
        self.width as usize * self.height as usize
    }
}

/// The §6.10 quantities.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub max_delta: u8,
    pub max_interior_delta: u8,
    pub over_count: u64,
    pub pct_over: f64,
    pub mae: f64,
    pub pixels: u64,
}

/// `compare` failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetricError {
    SizeMismatch { reference: (u32, u32), candidate: (u32, u32) },
}

impl fmt::Display for MetricError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SizeMismatch { reference, candidate } => write!(
                f,
                "size mismatch: reference {}x{}, candidate {}x{}",
                reference.0, reference.1, candidate.0, candidate.1
            ),
        }
    }
}

impl std::error::Error for MetricError {}

/// `(2126 r + 7152 g + 722 b) / 10000`, integer, no linearization.
pub fn luma(p: [u8; 4]) -> u32 {
    (2126 * u32::from(p[0]) + 7152 * u32::from(p[1]) + 722 * u32::from(p[2])) / 10000
}

/// Max channel difference.
pub fn delta(a: [u8; 4], b: [u8; 4]) -> u8 {
    (0..4).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0)
}

/// Sobel on `luma` (clamp-to-edge), `|Gx| + |Gy| > 64`, dilated by 1 px (3×3).
pub fn edge_mask(img: &Rgba8) -> Vec<bool> {
    let (w, h) = (img.width as i64, img.height as i64);
    let l: Vec<i64> =
        img.data.as_chunks::<4>().0.iter().map(|p| i64::from(luma([p[0], p[1], p[2], p[3]]))).collect();
    let at = |x: i64, y: i64| l[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize];
    let mut raw = vec![false; l.len()];
    for y in 0..h {
        for x in 0..w {
            let gx = at(x + 1, y - 1) + 2 * at(x + 1, y) + at(x + 1, y + 1)
                - at(x - 1, y - 1)
                - 2 * at(x - 1, y)
                - at(x - 1, y + 1);
            let gy = at(x - 1, y + 1) + 2 * at(x, y + 1) + at(x + 1, y + 1)
                - at(x - 1, y - 1)
                - 2 * at(x, y - 1)
                - at(x + 1, y - 1);
            raw[(y * w + x) as usize] = gx.abs() + gy.abs() > 64;
        }
    }
    let mut out = vec![false; l.len()];
    for y in 0..h {
        for x in 0..w {
            let hit = (-1..=1).any(|dy| {
                (-1..=1).any(|dx| {
                    let (nx, ny) = (x + dx, y + dy);
                    nx >= 0 && ny >= 0 && nx < w && ny < h && raw[(ny * w + nx) as usize]
                })
            });
            out[(y * w + x) as usize] = hit;
        }
    }
    out
}

/// `edge(r) OR edge(c)`.
pub fn combined_mask(r: &Rgba8, c: &Rgba8) -> Vec<bool> {
    edge_mask(r).into_iter().zip(edge_mask(c)).map(|(a, b)| a || b).collect()
}

/// The §6.10 metric of candidate `c` against reference `r`.
pub fn compare(r: &Rgba8, c: &Rgba8, over_threshold: u8) -> Result<Metrics, MetricError> {
    if (r.width, r.height) != (c.width, c.height) {
        return Err(MetricError::SizeMismatch {
            reference: (r.width, r.height),
            candidate: (c.width, c.height),
        });
    }
    let mask = combined_mask(r, c);
    let (mut max_delta, mut max_interior_delta, mut over_count, mut sum) = (0u8, 0u8, 0u64, 0u64);
    for (i, (a, b)) in r.data.as_chunks::<4>().0.iter().zip(c.data.as_chunks::<4>().0.iter()).enumerate() {
        let d = (0..4).map(|k| a[k].abs_diff(b[k])).max().unwrap_or(0);
        sum += (0..4).map(|k| u64::from(a[k].abs_diff(b[k]))).sum::<u64>();
        max_delta = max_delta.max(d);
        if !mask[i] {
            max_interior_delta = max_interior_delta.max(d);
        }
        if d > over_threshold {
            over_count += 1;
        }
    }
    let pixels = r.pixels() as u64;
    let n = pixels.max(1) as f64;
    Ok(Metrics {
        max_delta,
        max_interior_delta,
        over_count,
        pct_over: 100.0 * over_count as f64 / n,
        mae: sum as f64 / (4.0 * n),
        pixels,
    })
}

/// The transitional Smoke metric (§6.10).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmokeMetrics {
    pub max_tile_delta: f64,
    pub coverage_iou: f64,
}

impl SmokeMetrics {
    /// `max tile_delta ≤ 24` and `coverage_iou ≥ 0.97`.
    pub fn passes(&self) -> bool {
        self.max_tile_delta <= 24.0 && self.coverage_iou >= 0.97
    }
}

/// 8×8-tile mean delta and ink IoU against the premultiplied background `bg`.
pub fn smoke_compare(r: &Rgba8, c: &Rgba8, bg: [u8; 4]) -> Result<SmokeMetrics, MetricError> {
    if (r.width, r.height) != (c.width, c.height) {
        return Err(MetricError::SizeMismatch {
            reference: (r.width, r.height),
            candidate: (c.width, c.height),
        });
    }
    let mut max_tile_delta = 0.0f64;
    for ty in (0..r.height).step_by(8) {
        for tx in (0..r.width).step_by(8) {
            let (mut sr, mut sc, mut n) = ([0u64; 4], [0u64; 4], 0u64);
            for y in ty..(ty + 8).min(r.height) {
                for x in tx..(tx + 8).min(r.width) {
                    let (a, b) = (r.px(x, y), c.px(x, y));
                    for k in 0..4 {
                        sr[k] += u64::from(a[k]);
                        sc[k] += u64::from(b[k]);
                    }
                    n += 1;
                }
            }
            for k in 0..4 {
                let d = (sr[k] as f64 - sc[k] as f64).abs() / n as f64;
                max_tile_delta = max_tile_delta.max(d);
            }
        }
    }
    let ink = |p: &[u8]| (0..4).any(|k| p[k].abs_diff(bg[k]) > 8);
    let (mut inter, mut union) = (0u64, 0u64);
    for (a, b) in r.data.as_chunks::<4>().0.iter().zip(c.data.as_chunks::<4>().0.iter()) {
        let (ia, ib) = (ink(a), ink(b));
        inter += u64::from(ia && ib);
        union += u64::from(ia || ib);
    }
    let coverage_iou = if union == 0 { 1.0 } else { inter as f64 / union as f64 };
    Ok(SmokeMetrics { max_tile_delta, coverage_iou })
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: [u8; 4] = [255; 4];
    const BLACK: [u8; 4] = [0, 0, 0, 255];

    fn square(off: u32) -> Rgba8 {
        let mut img = Rgba8::filled(64, 64, WHITE);
        for y in 20 + off..40 + off {
            for x in 20 + off..40 + off {
                img.set(x, y, BLACK);
            }
        }
        img
    }

    #[test]
    fn identical_images_are_zero() {
        let m = compare(&square(0), &square(0), 0).unwrap();
        assert_eq!((m.max_delta, m.max_interior_delta, m.over_count), (0, 0, 0));
        assert_eq!((m.pct_over, m.mae), (0.0, 0.0));
    }

    #[test]
    fn single_pixel_change_counts_once() {
        let r = Rgba8::filled(16, 16, [0, 0, 0, 0]);
        let mut c = r.clone();
        c.set(5, 5, [255, 0, 0, 0]);
        let m = compare(&r, &c, 0).unwrap();
        assert_eq!((m.over_count, m.max_delta), (1, 255));
    }

    #[test]
    fn one_px_edge_shift_has_zero_interior_delta() {
        let m = compare(&square(0), &square(1), 2).unwrap();
        assert_eq!(m.max_interior_delta, 0);
        assert!(m.pct_over > 0.0);
    }

    #[test]
    fn size_mismatch_is_error() {
        let e = compare(&Rgba8::filled(2, 2, WHITE), &Rgba8::filled(3, 2, WHITE), 0).unwrap_err();
        assert_eq!(e, MetricError::SizeMismatch { reference: (2, 2), candidate: (3, 2) });
        assert!(e.to_string().contains("3x2"));
    }

    #[test]
    fn mae_of_uniform_offset() {
        let r = Rgba8::filled(8, 8, [10, 20, 30, 40]);
        let c = Rgba8::filled(8, 8, [14, 24, 34, 44]);
        assert_eq!(compare(&r, &c, 0).unwrap().mae, 4.0);
    }

    #[test]
    fn edge_mask_marks_rect_border_only() {
        let img = square(0);
        let m = edge_mask(&img);
        let at = |x: u32, y: u32| m[(y * 64 + x) as usize];
        assert!(at(20, 30) && at(39, 30) && at(30, 19) && at(30, 40));
        assert!(!at(30, 30), "interior");
        assert!(!at(5, 5) && !at(60, 60), "background");
    }

    #[test]
    fn smoke_ignores_aa_but_catches_missing_content() {
        assert!(smoke_compare(&square(0), &square(0), WHITE).unwrap().passes());
        let empty = Rgba8::filled(64, 64, WHITE);
        let s = smoke_compare(&square(0), &empty, WHITE).unwrap();
        assert!(!s.passes());
        assert_eq!(s.coverage_iou, 0.0);
    }

    #[test]
    fn luma_formula() {
        assert_eq!(luma(WHITE), 255);
        assert_eq!(luma([255, 0, 0, 255]), 54);
    }
}
