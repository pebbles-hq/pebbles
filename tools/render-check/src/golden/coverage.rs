//! Exact sub-scanline coverage of a filled path (05 §6.7a): flatten in device space at
//! `1/1024` px, 256 sub-scanlines per pixel row, exact x-interval integration per column.

use kurbo::{Affine, BezPath, PathEl, Point};
use peniko::Fill;

/// Device-space flattening tolerance (px).
pub const FLATTEN_TOL_PX: f64 = 1.0 / 1024.0;
/// Sub-scanlines per pixel row.
pub const SUBSCANLINES: u32 = 256;

/// A non-horizontal device-space edge, `y0 < y1`; `dir` is +1 downward, −1 upward.
#[derive(Clone, Copy, Debug)]
struct Edge {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    dir: i32,
}

impl Edge {
    fn new(a: Point, b: Point) -> Option<Self> {
        if a.y == b.y {
            return None;
        }
        Some(if a.y < b.y {
            Self { x0: a.x, y0: a.y, x1: b.x, y1: b.y, dir: 1 }
        } else {
            Self { x0: b.x, y0: b.y, x1: a.x, y1: a.y, dir: -1 }
        })
    }

    /// The x where the edge crosses `y` (caller guarantees `y0 <= y < y1`).
    fn x_at(&self, y: f64) -> f64 {
        self.x0 + (y - self.y0) * (self.x1 - self.x0) / (self.y1 - self.y0)
    }
}

/// Flatten `path` under `device` into edges; every subpath is implicitly closed (fill rule).
fn edges(path: &BezPath, device: Affine) -> Vec<Edge> {
    let device_path = device * path.clone();
    let mut out = Vec::new();
    let mut start: Option<Point> = None;
    let mut cur = Point::ZERO;
    let close = |from: Point, start: Option<Point>, out: &mut Vec<Edge>| {
        if let Some(s) = start {
            out.extend(Edge::new(from, s));
        }
    };
    kurbo::flatten(device_path.elements().iter().copied(), FLATTEN_TOL_PX, |el| match el {
        PathEl::MoveTo(p) => {
            close(cur, start, &mut out);
            start = Some(p);
            cur = p;
        }
        PathEl::LineTo(p) => {
            out.extend(Edge::new(cur, p));
            cur = p;
        }
        PathEl::ClosePath => {
            close(cur, start, &mut out);
            if let Some(s) = start {
                cur = s;
            }
            start = None;
        }
        // `flatten` emits only MoveTo / LineTo / ClosePath.
        PathEl::QuadTo(..) | PathEl::CurveTo(..) => {}
    });
    close(cur, start, &mut out);
    out
}

/// Add `weight · |[a, b] ∩ [i, i+1]|` to every column `i` of `row`.
fn add_span(row: &mut [f64], a: f64, b: f64, weight: f64) {
    let w = row.len() as f64;
    let (a, b) = (a.max(0.0), b.min(w));
    if b <= a {
        return;
    }
    let first = a.floor() as usize;
    let last = (b.ceil() as usize).min(row.len());
    for (i, cell) in row.iter_mut().enumerate().take(last).skip(first) {
        let x = i as f64;
        let overlap = b.min(x + 1.0) - a.max(x);
        if overlap > 0.0 {
            *cell += overlap * weight;
        }
    }
}

fn inside(rule: Fill, winding: i32) -> bool {
    match rule {
        Fill::NonZero => winding != 0,
        Fill::EvenOdd => winding % 2 != 0,
    }
}

/// Coverage in `[0, 1]` of every pixel of a `w × h` target (row-major) by `path` under
/// `device`, filled with `rule`. Exact horizontally; ≤ 1/512 error vertically.
pub fn fill_coverage(path: &BezPath, device: Affine, rule: Fill, w: u32, h: u32) -> Vec<f64> {
    let (wu, hu) = (w as usize, h as usize);
    let mut cov = vec![0.0; wu * hu];
    let all = edges(path, device);
    if all.iter().any(|e| ![e.x0, e.y0, e.x1, e.y1].iter().all(|v| v.is_finite())) {
        return cov;
    }
    let weight = 1.0 / f64::from(SUBSCANLINES);
    let mut row_edges: Vec<Edge> = Vec::new();
    let mut crossings: Vec<(f64, i32)> = Vec::new();
    for j in 0..hu {
        let (top, bottom) = (j as f64, j as f64 + 1.0);
        row_edges.clear();
        row_edges.extend(all.iter().filter(|e| e.y0 < bottom && e.y1 > top));
        if row_edges.is_empty() {
            continue;
        }
        let row = &mut cov[j * wu..(j + 1) * wu];
        for k in 0..SUBSCANLINES {
            let y = top + (f64::from(k) + 0.5) * weight;
            crossings.clear();
            crossings.extend(row_edges.iter().filter(|e| e.y0 <= y && y < e.y1).map(|e| (e.x_at(y), e.dir)));
            crossings.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut winding = 0;
            let mut span_start = 0.0;
            for &(x, dir) in &crossings {
                let was = inside(rule, winding);
                winding += dir;
                match (was, inside(rule, winding)) {
                    (false, true) => span_start = x,
                    (true, false) => add_span(row, span_start, x, weight),
                    _ => {}
                }
            }
        }
    }
    cov
}
