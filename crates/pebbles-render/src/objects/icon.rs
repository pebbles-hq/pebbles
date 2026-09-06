//! [`RenderIcon`] — paints an [`IconData`] glyph inside its box. The glyph's
//! primitives are authored in a `view`-unit square (24 for Lucide) and scaled to
//! the requested pixel size. The icon **model** (and the bundled Lucide set)
//! lives in `pebbles-icons`; this is only the painter.

pub use pebbles_icons::{IconData, IconKind, IconPrim, lucide};

use kurbo::{Affine, BezPath, Cap, Circle, Ellipse, Join, RoundedRect, Shape, Stroke};
use pebbles_foundation::{Color, Offset, Size};
use peniko::Fill;

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// A leaf render object that paints an [`IconData`].
pub struct RenderIcon {
    pub data: IconData,
    pub size: f64,
    pub color: Color,
}

impl RenderIcon {
    pub fn new(data: impl Into<IconData>, size: f64, color: Color) -> Self {
        RenderIcon { data: data.into(), size, color }
    }
}

/// Turn one primitive into a `kurbo` path in viewbox space.
fn prim_path(prim: &IconPrim) -> BezPath {
    match *prim {
        IconPrim::Path(d) => BezPath::from_svg(d).unwrap_or_default(),
        IconPrim::Line(x1, y1, x2, y2) => {
            let mut p = BezPath::new();
            p.move_to((x1, y1));
            p.line_to((x2, y2));
            p
        }
        IconPrim::Polyline(pts) => points_path(pts, false),
        IconPrim::Polygon(pts) => points_path(pts, true),
        IconPrim::Circle(cx, cy, r) => Circle::new((cx, cy), r).to_path(0.1),
        IconPrim::Ellipse(cx, cy, rx, ry) => Ellipse::new((cx, cy), (rx, ry), 0.0).to_path(0.1),
        IconPrim::Rect(x, y, w, h, rx, _ry) => {
            // kurbo rounds corners circularly; Lucide rects are effectively circular.
            RoundedRect::new(x, y, x + w, y + h, rx).to_path(0.1)
        }
    }
}

fn points_path(pts: &[(f64, f64)], close: bool) -> BezPath {
    let mut p = BezPath::new();
    for (i, &(x, y)) in pts.iter().enumerate() {
        if i == 0 {
            p.move_to((x, y));
        } else {
            p.line_to((x, y));
        }
    }
    if close && !pts.is_empty() {
        p.close_path();
    }
    p
}

impl RenderObject for RenderIcon {
    fn layout(&mut self, _cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        constraints.constrain(Size::new(self.size, self.size))
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let scale = self.size / self.data.view;
        let t = Affine::translate((offset.x, offset.y)) * Affine::scale(scale);
        let stroke = Stroke::new(self.data.stroke_width).with_caps(Cap::Round).with_join(Join::Round);

        for prim in self.data.prims {
            let path = prim_path(prim);
            if self.data.fill {
                cx.scene.fill(Fill::NonZero, t, self.color, None, &path);
            } else {
                cx.scene.stroke(&stroke, t, self.color, None, &path);
            }
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderIcon"
    }
}
