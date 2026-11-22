//! [`RenderFittedBox`] — lays its child out at its natural size, then scales and
//! positions it to fit (or cover) the box this object is given. Flutter's
//! `FittedBox` render object; the scale is expressed as a paint/hit-test
//! transform, so pointer events land on the child exactly where it appears.

use kurbo::Affine;
use pebbles_foundation::{Alignment, Axis, BoxFit, Offset, Rect, Size};
use peniko::{Fill, Mix};

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{IntrinsicCx, LayoutCx, PaintCx};

/// Scales its child per a [`BoxFit`] into the constraints it is handed.
pub struct RenderFittedBox {
    pub fit: BoxFit,
    pub alignment: Alignment,
    /// Scale + placement computed by the last layout pass, consumed by the
    /// paint/hit-test transform (the transform API only sees this box's own size,
    /// not the child's, so the factors must be remembered here).
    scale: (f64, f64),
    position: Offset,
}

impl RenderFittedBox {
    pub fn new(fit: BoxFit, alignment: Alignment) -> Self {
        RenderFittedBox { fit, alignment, scale: (1.0, 1.0), position: Offset::ZERO }
    }

    /// Flutter's `constrainSizeAndAttemptToPreserveAspectRatio`: fit `size` into
    /// `constraints` while preserving its aspect ratio as far as possible.
    fn constrain_preserving_aspect(constraints: BoxConstraints, size: Size) -> Size {
        if constraints.is_tight() {
            return constraints.smallest();
        }
        if size.width <= 0.0 || size.height <= 0.0 {
            return constraints.constrain(size);
        }
        let aspect = size.width / size.height;
        let mut width = size.width;
        let mut height = size.height;
        if width > constraints.max_width {
            width = constraints.max_width;
            height = width / aspect;
        }
        if height > constraints.max_height {
            height = constraints.max_height;
            width = height * aspect;
        }
        if width < constraints.min_width {
            width = constraints.min_width;
            height = width / aspect;
        }
        if height < constraints.min_height {
            height = constraints.min_height;
            width = height * aspect;
        }
        constraints.constrain(Size::new(width, height))
    }

    /// The scale that fits a `child`-sized child into a box of `size` (Flutter's
    /// `applyBoxFit`, measured against the box's final size — not the constraints).
    fn scale_for(&self, size: Size, child: Size) -> (f64, f64) {
        let cw = child.width;
        let ch = child.height;
        if cw <= 0.0 || ch <= 0.0 {
            return (1.0, 1.0);
        }
        let (bw, bh) = (size.width, size.height);
        match self.fit {
            BoxFit::Contain => {
                let s = (bw / cw).min(bh / ch);
                (s, s)
            }
            BoxFit::Cover => {
                let s = (bw / cw).max(bh / ch);
                (s, s)
            }
            BoxFit::Fill => (bw / cw, bh / ch),
            BoxFit::None => (1.0, 1.0),
            BoxFit::FitWidth => {
                let s = bw / cw;
                (s, s)
            }
            BoxFit::FitHeight => {
                let s = bh / ch;
                (s, s)
            }
            BoxFit::ScaleDown => {
                let s = (bw / cw).min(bh / ch).min(1.0);
                (s, s)
            }
        }
    }
}

impl RenderObject for RenderFittedBox {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        let Some(child) = cx.children().first().copied() else {
            return constraints.constrain(constraints.biggest());
        };
        // The child sizes itself with no constraint on either axis (natural size).
        cx.layout_child(child, BoxConstraints::UNBOUNDED);
        let child_size = cx.child_size(child);
        // Flutter: the box is the child's natural size fitted into the constraints with
        // its aspect preserved; the child is then scaled into that box and aligned.
        let size = Self::constrain_preserving_aspect(constraints, child_size);
        let (sx, sy) = self.scale_for(size, child_size);
        // Where the scaled child's top-left lands within the box (alignment -1..1).
        let dw = size.width - child_size.width * sx;
        let dh = size.height - child_size.height * sy;
        self.position = Offset::new(dw * (self.alignment.x + 1.0) / 2.0, dh * (self.alignment.y + 1.0) / 2.0);
        self.scale = (sx, sy);
        cx.set_child_offset(child, Offset::ZERO);
        size
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        if let Some(child) = cx.children().first().copied() {
            // Clip the scaled child to this box — Cover / None / Fill can paint the
            // child larger than the box, and Flutter's FittedBox clips that overflow
            // (`clipBehavior: Clip.hardEdge`). The clip is a rect in the box's own
            // space; the child's scale transform applies inside `paint_child_clipped`.
            // This box paints under its own transform (translate(position) · scale), so
            // its clip is the box's area in that scaled space: size / scale, shifted
            // back by the placement — not the box size, which the scale would shrink
            // a second time (clipping the scaled child's right / bottom).
            let (sx, sy) = self.scale;
            let size = cx.size();
            let (sx, sy) = (if sx.abs() > 1e-9 { sx } else { 1.0 }, if sy.abs() > 1e-9 { sy } else { 1.0 });
            let bounds = Rect::new(
                offset.x - self.position.x / sx,
                offset.y - self.position.y / sy,
                offset.x + (size.width - self.position.x) / sx,
                offset.y + (size.height - self.position.y) / sy,
            );
            cx.scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &bounds);
            cx.paint_child_clipped(child, offset, bounds);
            cx.scene.pop_layer();
        }
    }

    fn clips_children(&self) -> bool {
        // The clip layer bounds the (scaled) subtree's ink to this box.
        true
    }

    fn transform(&self, _size: Size) -> Option<Affine> {
        let (sx, sy) = self.scale;
        let pos = self.position;
        if (sx - 1.0).abs() < 1e-9 && (sy - 1.0).abs() < 1e-9 && pos == Offset::ZERO {
            None
        } else {
            Some(Affine::translate((pos.x, pos.y)) * Affine::scale_non_uniform(sx, sy))
        }
    }

    fn intrinsic(&self, cx: &mut IntrinsicCx<'_>, axis: Axis, cross_extent: f64) -> Option<f64> {
        // A fitted box's intrinsic extent is its child's — scaling is a paint-time
        // concern, not an intrinsic one.
        cx.children().first().copied().and_then(|child| cx.child_intrinsic(child, axis, cross_extent))
    }

    fn debug_name(&self) -> &'static str {
        "RenderFittedBox"
    }
}
