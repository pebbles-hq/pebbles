//! Tier B path masks (02 §5.2, R14): CPU area-coverage rasterization with zeno, swash's own
//! rasterizer, so path and glyph edges get identical coverage (DR-23).

use kurbo::{Affine, BezPath, PathEl, Point};
use zeno::{Command, Format, Mask, Scratch};

use crate::prepare::ScissorRect;
use crate::prepare::num::{ceil_px, floor_px, narrow_f32};

/// An 8-bit coverage mask, row-major, `width × height` bytes.
#[derive(Debug, Default)]
pub(crate) struct MaskBuf {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Device-pixel origin and size of a rasterized mask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MaskPlacement {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
}

/// Device-space flattening tolerance for curves (px).
pub(crate) const FLATTEN_TOL_PX: f64 = 1.0 / 512.0;

/// Reusable rasterizer state; capacities never shrink below their high-water mark.
#[derive(Default)]
pub(crate) struct CoverageRasterizer {
    cmds: Vec<Command>,
    scratch: Scratch,
}

impl std::fmt::Debug for CoverageRasterizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoverageRasterizer").field("cmds", &self.cmds.len()).finish_non_exhaustive()
    }
}

/// Device-space bounds of the control points (a superset of the curve), `None` without points
/// or with a non-finite one.
fn device_bounds(path: &BezPath, device: Affine) -> Option<kurbo::Rect> {
    let mut b: Option<kurbo::Rect> = None;
    for el in path.elements() {
        let pts: &[Point] = match el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => std::slice::from_ref(p),
            PathEl::QuadTo(a, p) => &[*a, *p],
            PathEl::CurveTo(a, c, p) => &[*a, *c, *p],
            PathEl::ClosePath => &[],
        };
        for &p in pts {
            let q = device * p;
            if !(q.x.is_finite() && q.y.is_finite()) {
                return None;
            }
            b = Some(b.map_or(kurbo::Rect::from_points(q, q), |r| r.union_pt(q)));
        }
    }
    b
}

/// Mask-space lines clipped to the box `[0, w] × [0, h]`: each segment is split where it
/// crosses a box edge and the outside pieces are clamped onto the box. A piece left of the box
/// becomes a vertical edge at `x = 0` carrying the same winding into the visible pixels (zeno
/// drops edges left of its buffer); pieces above, below or right of it become edges that cover
/// nothing. Exact, and it bounds the coordinates zeno sees.
struct ClampedPath<'a> {
    cmds: &'a mut Vec<Command>,
    w: f64,
    h: f64,
    /// The unclamped start of the open subpath.
    start: Option<Point>,
    cur: Point,
}

impl ClampedPath<'_> {
    /// Clamp onto the box, then add half of zeno's 1/256 px fixed-point step: zeno truncates
    /// coordinates (`to_fixed`), so this makes its conversion round to nearest.
    fn clamp(&self, p: Point) -> zeno::Point {
        const HALF_STEP: f64 = 0.5 / 256.0;
        let x = p.x.clamp(0.0, self.w) + HALF_STEP;
        let y = p.y.clamp(0.0, self.h) + HALF_STEP;
        zeno::Point::new(narrow_f32(x), narrow_f32(y))
    }

    fn move_to(&mut self, p: Point) {
        self.close();
        self.start = Some(p);
        self.cur = p;
        self.cmds.push(Command::MoveTo(self.clamp(p)));
    }

    fn line_to(&mut self, p: Point) {
        let a = self.cur;
        let d = p - a;
        let mut ts = [1.0f64; 4];
        for (k, (lo, v)) in
            [(a.x, d.x), (a.x - self.w, d.x), (a.y, d.y), (a.y - self.h, d.y)].into_iter().enumerate()
        {
            let t = -lo / v;
            if v != 0.0 && t > 0.0 && t < 1.0 {
                ts[k] = t;
            }
        }
        ts.sort_by(f64::total_cmp);
        for t in ts.into_iter().filter(|&t| t < 1.0) {
            let q = a + d * t;
            self.cmds.push(Command::LineTo(self.clamp(q)));
        }
        self.cmds.push(Command::LineTo(self.clamp(p)));
        self.cur = p;
    }

    /// Close the open subpath with a clipped closing line.
    fn close(&mut self) {
        if let Some(s) = self.start.take() {
            if s != self.cur {
                self.line_to(s);
            }
            self.cmds.push(Command::Close);
            self.cur = s;
        }
    }
}

impl CoverageRasterizer {
    /// Rasterize `path` under `device` with `rule`, limited to `clip`, into `out`. `None` for an
    /// empty intersection, a non-finite point (R02 counts those) or a path with no area.
    pub(crate) fn rasterize(
        &mut self,
        path: &BezPath,
        device: Affine,
        rule: peniko::Fill,
        clip: ScissorRect,
        out: &mut MaskBuf,
    ) -> Option<MaskPlacement> {
        let b = device_bounds(path, device)?;
        if b.width() <= 0.0 || b.height() <= 0.0 {
            return None;
        }
        let x0 = floor_px(b.x0, clip.x1)?.max(clip.x0);
        let y0 = floor_px(b.y0, clip.y1)?.max(clip.y0);
        let x1 = ceil_px(b.x1, clip.x1)?.max(clip.x0);
        let y1 = ceil_px(b.y1, clip.y1)?.max(clip.y0);
        if x1 <= x0 || y1 <= y0 {
            return None;
        }
        let (w, h) = (x1 - x0, y1 - y0);
        let m = Affine::translate((-f64::from(x0), -f64::from(y0))) * device;
        // Curves are flattened here at `FLATTEN_TOL_PX` (chords sit inside the curve, so the error is
        // a one-sided ≤ 1/512 px ≈ 0.5 LSB; zeno's own subdivision loses ~0.2 % of
        // a circle's area, beyond the ≤ 1 LSB budget of 02 §5.2); every line is then clipped to
        // the mask box (see `ClampedPath`).
        self.cmds.clear();
        let mut out_path = ClampedPath {
            cmds: &mut self.cmds,
            w: f64::from(w),
            h: f64::from(h),
            start: None,
            cur: Point::ZERO,
        };
        kurbo::flatten(path.elements().iter().map(|el| m * *el), FLATTEN_TOL_PX, |el| match el {
            PathEl::MoveTo(a) => out_path.move_to(a),
            PathEl::LineTo(a) | PathEl::QuadTo(_, a) | PathEl::CurveTo(_, _, a) => out_path.line_to(a),
            PathEl::ClosePath => out_path.close(),
        });
        out_path.close();
        let fill = match rule {
            peniko::Fill::NonZero => zeno::Fill::NonZero,
            peniko::Fill::EvenOdd => zeno::Fill::EvenOdd,
        };
        let len = usize::try_from(u64::from(w) * u64::from(h)).ok()?;
        out.data.clear();
        out.data.resize(len, 0);
        out.width = w;
        out.height = h;
        Mask::with_scratch(&self.cmds, &mut self.scratch)
            .style(fill)
            .format(Format::Alpha)
            .size(w, h)
            .render_into(&mut out.data, None);
        let left = i32::try_from(x0).ok()?;
        let top = i32::try_from(y0).ok()?;
        Some(MaskPlacement { left, top, width: w, height: h })
    }

    /// `(cmds, …)` capacity, for the no-allocation-after-warm-up test.
    #[cfg(test)]
    pub(crate) fn cmds_capacity(&self) -> usize {
        self.cmds.capacity()
    }
}

#[cfg(test)]
#[path = "coverage_tests.rs"]
mod tests;

/// R14's seeded oracle check (render-check `tests/tier_b_vs_golden.rs`): the full `w × h`
/// mask of `path` under `device`. Not API.
pub(crate) fn full_mask(path: &BezPath, device: Affine, rule: peniko::Fill, w: u32, h: u32) -> Vec<u8> {
    let mut out = MaskBuf::default();
    let mut full = vec![0u8; usize::try_from(u64::from(w) * u64::from(h)).unwrap_or(0)];
    let clip = ScissorRect { x0: 0, y0: 0, x1: w, y1: h };
    if let Some(p) = CoverageRasterizer::default().rasterize(path, device, rule, clip, &mut out) {
        let (left, top) = (p.left.unsigned_abs(), p.top.unsigned_abs());
        for (row, src) in out.data.chunks_exact(p.width as usize).enumerate() {
            let y = top as usize + row;
            let start = y * w as usize + left as usize;
            full[start..start + src.len()].copy_from_slice(src);
        }
    }
    full
}
