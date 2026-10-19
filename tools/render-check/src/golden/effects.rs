//! Layer effects, backdrop filters and projected layers in the golden model (09 §10, V52),
//! written from the spec text: every pass quantizes to 8 bits like the renderer's Rgba8
//! intermediates. Images are premultiplied RGBA `f64` over a `w × h` rect.

use kurbo::{Affine, Point};
use pebbles_render::paint::Projective;
use pebbles_render::paint::display_list::{BlurEdge, LayerEffect};

/// One device-space step (09 §10.1: blur σ and morphology radii scale by `s`).
#[derive(Clone, Copy, Debug)]
pub enum Step {
    Blur { sx: f64, sy: f64, clamp: bool },
    Matrix([f32; 20]),
    Dilate { rx: usize, ry: usize },
    Erode { rx: usize, ry: usize },
}

pub fn steps(effect: &LayerEffect, s: f64, out: &mut Vec<Step>) {
    let px = |v: f32| (f64::from(v.max(0.0)) * s).round().min(4096.0) as usize;
    match effect {
        LayerEffect::Blur { sigma_x, sigma_y, edge } => out.push(Step::Blur {
            sx: f64::from((f64::from(sigma_x.max(0.0)) * s) as f32),
            sy: f64::from((f64::from(sigma_y.max(0.0)) * s) as f32),
            clamp: *edge == BlurEdge::Clamp,
        }),
        LayerEffect::ColorMatrix(m) => out.push(Step::Matrix(*m)),
        LayerEffect::Dilate { radius_x, radius_y } => {
            out.push(Step::Dilate { rx: px(*radius_x), ry: px(*radius_y) })
        }
        LayerEffect::Erode { radius_x, radius_y } => {
            out.push(Step::Erode { rx: px(*radius_x), ry: px(*radius_y) })
        }
        LayerEffect::Chain(es) => es.iter().for_each(|e| steps(e, s, out)),
        _ => {}
    }
}

fn q(v: f64) -> f64 {
    (v.clamp(0.0, 1.0) * 255.0).round() / 255.0
}

pub fn quantize(img: &mut [[f64; 4]]) {
    for p in img.iter_mut() {
        *p = p.map(q);
    }
}

#[derive(Clone)]
pub struct Img {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[f64; 4]>,
}

impl Img {
    fn at_clamped(&self, x: i64, y: i64) -> [f64; 4] {
        let x = x.clamp(0, self.w as i64 - 1) as usize;
        let y = y.clamp(0, self.h as i64 - 1) as usize;
        self.px[y * self.w + x]
    }
    fn at(&self, x: i64, y: i64, clamp: bool) -> [f64; 4] {
        if clamp || (x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h) {
            self.at_clamped(x, y)
        } else {
            [0.0; 4]
        }
    }
}

fn add(a: [f64; 4], b: [f64; 4], k: f64) -> [f64; 4] {
    [a[0] + b[0] * k, a[1] + b[1] * k, a[2] + b[2] * k, a[3] + b[3] * k]
}

fn weights(sigma: f64) -> Vec<f64> {
    let r = ((3.0 * sigma as f32).ceil() as i64).min(27);
    let full: Vec<f64> = (-r..=r).map(|i| (-((i * i) as f64) / (2.0 * sigma * sigma)).exp()).collect();
    let sum: f64 = full.iter().sum();
    full[r as usize..].iter().map(|v| v / sum).collect()
}

fn blur(img: Img, sx: f64, sy: f64, clamp: bool) -> Img {
    if sx <= 0.0 && sy <= 0.0 {
        return img;
    }
    let smax = sx.max(sy);
    let mut k = 0;
    while k < 3 && (smax as f32) / (1u32 << k) as f32 > 2.0 {
        k += 1;
    }
    let d = f64::from(1u32 << k);
    let mut levels = vec![img];
    for _ in 0..k {
        let l = levels.last().cloned().unwrap();
        let (w, h) = (l.w.div_ceil(2).max(1), l.h.div_ceil(2).max(1));
        let mut px = Vec::with_capacity(w * h);
        for y in 0..h as i64 {
            for x in 0..w as i64 {
                let s = [(0, 0), (1, 0), (0, 1), (1, 1)]
                    .iter()
                    .fold([0.0; 4], |acc, (dx, dy)| add(acc, l.at_clamped(2 * x + dx, 2 * y + dy), 0.25));
                px.push(s.map(q));
            }
        }
        levels.push(Img { w, h, px });
    }
    let mut cur = levels.pop().unwrap();
    for (axis, s) in [(0, sx / d), (1, sy / d)] {
        if s <= 0.0 {
            continue;
        }
        let wts = weights(s);
        let src = cur.clone();
        for y in 0..cur.h as i64 {
            for x in 0..cur.w as i64 {
                let mut acc = [0.0; 4];
                for (i, wt) in wts.iter().enumerate() {
                    let i = i as i64;
                    let (a, b) = if axis == 0 { ((x + i, y), (x - i, y)) } else { ((x, y + i), (x, y - i)) };
                    acc = add(acc, src.at(a.0, a.1, clamp), *wt);
                    if i > 0 {
                        acc = add(acc, src.at(b.0, b.1, clamp), *wt);
                    }
                }
                cur.px[y as usize * cur.w + x as usize] = acc.map(q);
            }
        }
    }
    while let Some(target) = levels.pop() {
        let mut px = Vec::with_capacity(target.w * target.h);
        for y in 0..target.h {
            for x in 0..target.w {
                let (u, v) = ((x as f64 + 0.5) / 2.0 - 0.5, (y as f64 + 0.5) / 2.0 - 0.5);
                let (x0, y0) = (u.floor(), v.floor());
                let (fx, fy) = (u - x0, v - y0);
                let (i, j) = (x0 as i64, y0 as i64);
                let top = add(cur.at_clamped(i, j).map(|c| c * (1.0 - fx)), cur.at_clamped(i + 1, j), fx);
                let bot =
                    add(cur.at_clamped(i, j + 1).map(|c| c * (1.0 - fx)), cur.at_clamped(i + 1, j + 1), fx);
                px.push(add(top.map(|c| c * (1.0 - fy)), bot, fy).map(q));
            }
        }
        cur = Img { w: target.w, h: target.h, px };
    }
    cur
}

fn matrix(mut img: Img, m: &[f32; 20]) -> Img {
    for p in &mut img.px {
        let a = p[3];
        let c = if a > 0.0 { [p[0] / a, p[1] / a, p[2] / a] } else { [0.0; 3] };
        let x = [c[0], c[1], c[2], a, 1.0];
        let v: [f64; 4] = std::array::from_fn(|row| {
            (0..5).map(|k| f64::from(m[row * 5 + k]) * x[k]).sum::<f64>().clamp(0.0, 1.0)
        });
        *p = [v[0] * v[3], v[1] * v[3], v[2] * v[3], v[3]].map(q);
    }
    img
}

fn morph(img: Img, rx: usize, ry: usize, dilate: bool) -> Img {
    let pick = |a: f64, b: f64| if dilate { a.max(b) } else { a.min(b) };
    let mut cur = img;
    for (axis, r) in [(0, rx), (1, ry)] {
        if r == 0 {
            continue;
        }
        let src = cur.clone();
        for y in 0..cur.h as i64 {
            for x in 0..cur.w as i64 {
                let mut acc = src.at(x, y, false);
                for i in 1..=r as i64 {
                    for (a, b) in if axis == 0 { [(x + i, y), (x - i, y)] } else { [(x, y + i), (x, y - i)] }
                    {
                        let s = src.at(a, b, false);
                        acc = std::array::from_fn(|k| pick(acc[k], s[k]));
                    }
                }
                cur.px[y as usize * cur.w + x as usize] = acc;
            }
        }
    }
    cur
}

/// Run `steps` over `img` (already 8-bit), forcing Clamp edges when `clamp_all`.
pub fn apply(mut img: Img, steps: &[Step], clamp_all: bool) -> Img {
    for s in steps {
        img = match *s {
            Step::Blur { sx, sy, clamp } => blur(img, sx, sy, clamp || clamp_all),
            Step::Matrix(m) => matrix(img, &m),
            Step::Dilate { rx, ry } => morph(img, rx, ry, true),
            Step::Erode { rx, ry } => morph(img, rx, ry, false),
        };
    }
    img
}

/// The device rect `(x0, y0, x1, y1)` of `img` inside a `w`-wide buffer: crop / write back.
pub fn crop(buf: &[[f64; 4]], w: usize, r: (usize, usize, usize, usize)) -> Img {
    let (x0, y0, x1, y1) = r;
    let mut px = Vec::with_capacity((x1 - x0) * (y1 - y0));
    for y in y0..y1 {
        px.extend_from_slice(&buf[y * w + x0..y * w + x1]);
    }
    Img { w: x1 - x0, h: y1 - y0, px }
}

/// Bilinear sample of `img` at texel-space `p` (centers at +0.5), transparent outside.
pub fn sample(img: &Img, p: Point) -> [f64; 4] {
    let (u, v) = (p.x - 0.5, p.y - 0.5);
    let (x0, y0) = (u.floor(), v.floor());
    let (fx, fy) = (u - x0, v - y0);
    let (i, j) = (x0 as i64, y0 as i64);
    let t = add(img.at(i, j, false).map(|c| c * (1.0 - fx)), img.at(i + 1, j, false), fx);
    let b = add(img.at(i, j + 1, false).map(|c| c * (1.0 - fx)), img.at(i + 1, j + 1, false), fx);
    add(t.map(|c| c * (1.0 - fy)), b, fy)
}

/// The §10.7 layer scale ρ and texture size for `bounds` under the device homography.
pub fn projected_scale(h_dev: &Projective, bounds: kurbo::Rect, max_side: f64) -> (f64, usize, usize) {
    let rho =
        [(bounds.x0, bounds.y0), (bounds.x1, bounds.y0), (bounds.x0, bounds.y1), (bounds.x1, bounds.y1)]
            .iter()
            .filter_map(|&(x, y)| h_dev.local_scale(Point::new(x, y)))
            .fold(0.0f64, f64::max)
            .clamp(0.25, 2.0);
    let rho = rho.min(max_side / bounds.width().max(1.0)).min(max_side / bounds.height().max(1.0));
    let side = |v: f64| ((v * rho).ceil() as usize).max(1);
    (rho, side(bounds.width()), side(bounds.height()))
}

/// Analytic edge coverage `clamp(0.5 + d, 0, 1)` of `p` against the quad `c` (strip order).
pub fn edge_coverage(p: Point, c: [Point; 4]) -> f64 {
    let mid =
        Point::new((c[0].x + c[1].x + c[2].x + c[3].x) / 4.0, (c[0].y + c[1].y + c[2].y + c[3].y) / 4.0);
    let edge = |a: Point, b: Point| {
        let d = b - a;
        let mut n = kurbo::Vec2::new(-d.y, d.x) / d.hypot().max(1e-12);
        if (mid - a).dot(n) < 0.0 {
            n = -n;
        }
        (p - a).dot(n)
    };
    let d = edge(c[0], c[1]).min(edge(c[1], c[3])).min(edge(c[3], c[2])).min(edge(c[2], c[0]));
    (0.5 + d).clamp(0.0, 1.0)
}

pub fn spectral(a: Affine) -> f64 {
    let [a0, b, c, d, ..] = a.as_coeffs();
    let t = a0 * a0 + b * b + c * c + d * d;
    let det = a0 * d - b * c;
    ((t + (t * t - 4.0 * det * det).max(0.0).sqrt()) / 2.0).sqrt()
}
