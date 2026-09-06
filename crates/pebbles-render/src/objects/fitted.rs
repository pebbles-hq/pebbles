//! [`RenderFittedBox`] — lays its child out at its natural size, then scales and
//! positions it to fit (or cover) the box this object is given. Flutter's
//! `FittedBox` render object; the scale is expressed as a paint/hit-test
//! transform, so pointer events land on the child exactly where it appears.

use pebbles_foundation::{Alignment, Axis, BoxFit, Offset, Size};
use kurbo::Affine;

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

    /// The scale applied to the child for this box.
    fn scale_for(&self, constraints: BoxConstraints, child: Size) -> (f64, f64) {
        let cw = child.width;
        let ch = child.height;
        if cw <= 0.0 || ch <= 0.0 {
            return (1.0, 1.0);
        }
        match self.fit {
            BoxFit::Contain => {
                let s = (constraints.max_width / cw).min(constraints.max_height / ch);
                (s, s)
            }
            BoxFit::Cover => {
                let s = (constraints.max_width / cw).max(constraints.max_height / ch);
                (s, s)
            }
            BoxFit::Fill => (constraints.max_width / cw, constraints.max_height / ch),
            BoxFit::None => (1.0, 1.0),
            BoxFit::FitWidth => {
                let s = constraints.max_width / cw;
                (s, s)
            }
            BoxFit::FitHeight => {
                let s = constraints.max_height / ch;
                (s, s)
            }
            BoxFit::ScaleDown => {
                let contain = (constraints.max_width / cw).min(constraints.max_height / ch);
                let s = contain.min(1.0);
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
        let (sx, sy) = self.scale_for(constraints, child_size);
        // The box wants to be the scaled child, kept inside the constraints while
        // preserving the child's aspect (Flutter semantics).
        let size = Self::constrain_preserving_aspect(
            constraints,
            Size::new(child_size.width * sx, child_size.height * sy),
        );
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
            cx.paint_child(child, offset);
        }
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
