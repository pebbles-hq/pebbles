//! Rect-clip classification (scissor) and the clip stack.

use kurbo::Affine;

use super::Tess;

/// An axis-aligned clip rectangle in physical-pixel space, for `set_scissor_rect`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct Scissor {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Scissor {
    fn intersect(self, other: Scissor) -> Scissor {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = (self.x + self.width).min(other.x + other.width);
        let y1 = (self.y + self.height).min(other.y + other.height);
        if x1 <= x0 || y1 <= y0 {
            Scissor { x: 0, y: 0, width: 0, height: 0 }
        } else {
            Scissor { x: x0, y: y0, width: x1 - x0, height: y1 - y0 }
        }
    }
}

impl Tess {
    pub(crate) fn push_clip(&mut self, scissor: Option<Scissor>) {
        self.clip_stack.push(self.scissor);
        self.scissor = match (self.scissor, scissor) {
            (None, None) => None,
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (Some(a), Some(b)) => Some(a.intersect(b)),
        };
    }

    pub(crate) fn pop_clip(&mut self) {
        self.scissor = self.clip_stack.pop().unwrap_or(None);
    }
}

/// If `path` (a recorded rect clip) maps to another axis-aligned rectangle under
/// `transform` (translate + scale, no rotation/shear), return its physical-pixel
/// scissor rect; otherwise `None` (non-rect clips are Phase 4's job).
pub(crate) fn scissor_rect(path: &kurbo::BezPath, transform: Affine) -> Option<Scissor> {
    let elements = path.elements();
    if elements.len() != 5 {
        return None;
    }
    let corner = |el: &kurbo::PathEl| -> Option<kurbo::Point> {
        match *el {
            kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) => Some(p),
            _ => None,
        }
    };
    let p0 = corner(&elements[0])?;
    let p1 = corner(&elements[1])?;
    let p2 = corner(&elements[2])?;
    let p3 = corner(&elements[3])?;
    if !matches!(elements[4], kurbo::PathEl::ClosePath) {
        return None;
    }
    let a = transform * p0;
    let b = transform * p1;
    let c = transform * p2;
    let d = transform * p3;
    const EPS: f64 = 1e-3;
    if (a.y - b.y).abs() > EPS
        || (c.y - d.y).abs() > EPS
        || (b.x - c.x).abs() > EPS
        || (d.x - a.x).abs() > EPS
    {
        return None;
    }
    let min_x = a.x.min(b.x).min(c.x).min(d.x);
    let max_x = a.x.max(b.x).max(c.x).max(d.x);
    let min_y = a.y.min(b.y).min(c.y).min(d.y);
    let max_y = a.y.max(b.y).max(c.y).max(d.y);
    Some(Scissor {
        x: min_x.floor().max(0.0) as u32,
        y: min_y.floor().max(0.0) as u32,
        width: (max_x.ceil() - min_x.floor()).max(0.0) as u32,
        height: (max_y.ceil() - min_y.floor()).max(0.0) as u32,
    })
}
