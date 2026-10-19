//! Box shadows in the golden model (05 §6.7a, V46 step 1): the rounded rect's exact coverage on a
//! 4× supersampled local grid, convolved with a 2-D Gaussian (truncated at 4σ; applied
//! separably), then sampled through the op transform at 4 × 4 subsamples per pixel.
//!
//! σ = `std_dev`: the true Gaussian standard deviation (Skia/Flutter semantics, 02 §8.2).

use kurbo::{Affine, Point, Rect, RoundedRect, Shape};
use peniko::Fill;

use super::coverage::fill_coverage;

/// Grid cells per local unit.
const SS: f64 = 4.0;

/// A normalized 1-D Gaussian kernel for `sigma` in grid cells, truncated at 4σ.
fn kernel(sigma_cells: f64) -> Vec<f64> {
    if sigma_cells < 1e-3 {
        return vec![1.0];
    }
    let r = (4.0 * sigma_cells).ceil() as i64;
    let k: Vec<f64> =
        (-r..=r).map(|i| (-((i * i) as f64) / (2.0 * sigma_cells * sigma_cells)).exp()).collect();
    let s: f64 = k.iter().sum();
    k.into_iter().map(|v| v / s).collect()
}

fn blur_1d(src: &[f64], dst: &mut [f64], n: usize, stride: usize, count: usize, step: usize, k: &[f64]) {
    let r = k.len() / 2;
    for line in 0..count {
        let base = line * step;
        for i in 0..n {
            let mut acc = 0.0;
            for (j, w) in k.iter().enumerate() {
                let t = i as i64 + j as i64 - r as i64;
                if t >= 0 && (t as usize) < n {
                    acc += w * src[base + t as usize * stride];
                }
            }
            dst[base + i * stride] = acc;
        }
    }
}

/// The blurred coverage field of `rect` (radius `radius`, σ `std_dev`, local units).
struct Field {
    origin: Point,
    /// Grid cells per local unit.
    ss: f64,
    w: usize,
    h: usize,
    v: Vec<f64>,
}

impl Field {
    fn new(rect: Rect, radius: f64, std_dev: f64) -> Option<Self> {
        // A whole-unit pad keeps cell centers on the lattice of an integer-aligned rect.
        let pad = (4.0 * std_dev).max(0.0).ceil() + 1.0;
        let area = rect.inflate(pad, pad);
        let origin = Point::new(area.x0, area.y0);
        // 4 cells per unit, coarsened for wide blurs so a cell stays ≤ σ/16 (the Gaussian is
        // smooth at that scale; cost stays bounded for σ = 64).
        let ss = if std_dev > 4.0 { (SS * 4.0 / std_dev).max(0.25) } else { SS };
        let (w, h) = ((area.width() * ss).ceil() as usize, (area.height() * ss).ceil() as usize);
        if w == 0 || h == 0 || w * h > 64_000_000 {
            return None;
        }
        let rr = RoundedRect::from_rect(rect, radius.max(0.0));
        let to_grid = Affine::scale(ss) * Affine::translate((-origin.x, -origin.y));
        let path = rr.to_path(0.01 / ss);
        let ind = fill_coverage(&path, to_grid, Fill::NonZero, w as u32, h as u32);
        let k = kernel(std_dev * ss);
        let mut tmp = vec![0.0; w * h];
        blur_1d(&ind, &mut tmp, w, 1, h, w, &k);
        let mut v = vec![0.0; w * h];
        blur_1d(&tmp, &mut v, h, w, w, 1, &k);
        Some(Self { origin, ss, w, h, v })
    }

    /// Bilinear lookup at local `p` (cell centers at (i + 0.5) / ss).
    fn at(&self, p: Point) -> f64 {
        let gx = (p.x - self.origin.x) * self.ss - 0.5;
        let gy = (p.y - self.origin.y) * self.ss - 0.5;
        let (x0, y0) = (gx.floor(), gy.floor());
        let (fx, fy) = (gx - x0, gy - y0);
        let get = |x: f64, y: f64| {
            if x < 0.0 || y < 0.0 || x >= self.w as f64 || y >= self.h as f64 {
                0.0
            } else {
                self.v[y as usize * self.w + x as usize]
            }
        };
        let a = get(x0, y0) * (1.0 - fx) + get(x0 + 1.0, y0) * fx;
        let b = get(x0, y0 + 1.0) * (1.0 - fx) + get(x0 + 1.0, y0 + 1.0) * fx;
        a * (1.0 - fy) + b * fy
    }
}

/// Coverage of a blurred rounded rect under `t` over a `w × h` target.
pub fn shadow_coverage(t: Affine, rect: Rect, radius: f64, std_dev: f64, w: u32, h: u32) -> Vec<f64> {
    let mut out = vec![0.0; w as usize * h as usize];
    let std_dev = std_dev.max(0.0);
    let Some(field) = Field::new(rect.abs(), radius, std_dev) else { return out };
    if t.determinant().abs() < 1e-12 {
        return out;
    }
    let inv = t.inverse();
    let pad = (4.0 * std_dev).max(0.0).ceil() + 1.0;
    let dev = t.transform_rect_bbox(rect.abs().inflate(pad, pad));
    let x0 = dev.x0.floor().max(0.0) as u32;
    let y0 = dev.y0.floor().max(0.0) as u32;
    let x1 = (dev.x1.ceil().max(0.0) as u32).min(w);
    let y1 = (dev.y1.ceil().max(0.0) as u32).min(h);
    for y in y0..y1 {
        for x in x0..x1 {
            let mut acc = 0.0;
            for sy in 0..4 {
                for sx in 0..4 {
                    let p = Point::new(
                        f64::from(x) + (f64::from(sx) + 0.5) / 4.0,
                        f64::from(y) + (f64::from(sy) + 0.5) / 4.0,
                    );
                    acc += field.at(inv * p);
                }
            }
            out[(y * w + x) as usize] = acc / 16.0;
        }
    }
    out
}
