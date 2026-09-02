//! [`RenderFractionallySizedBox`] — sizes its child to a fraction of the incoming
//! constraints (Flutter's `FractionallySizedBox`), aligning it within the box.

use pebbles_foundation::{Alignment, Offset, Size};
use vello::kurbo::Affine;

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// Sizes its child to `width_factor` × max width and `height_factor` × max height
/// of the incoming constraints (`None` passes that axis through unchanged).
pub struct RenderFractionallySizedBox {
    pub width_factor: Option<f64>,
    pub height_factor: Option<f64>,
    pub alignment: Alignment,
    /// Child position computed by the last layout pass (consumed by the paint
    /// transform, which only sees this box's own size).
    position: Offset,
}

impl RenderFractionallySizedBox {
    pub fn new(
        width_factor: Option<f64>,
        height_factor: Option<f64>,
        alignment: Alignment,
    ) -> Self {
        RenderFractionallySizedBox {
            width_factor,
            height_factor,
            alignment,
            position: Offset::ZERO,
        }
    }
}

impl RenderObject for RenderFractionallySizedBox {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        let Some(child) = cx.children().first().copied() else {
            return constraints.constrain(constraints.biggest());
        };
        // Tighten the set axes to the requested fraction, pass the other axis
        // through, then enforce the result back inside the incoming constraints
        // (Flutter's `tightFor(width:, height:).enforce(constraints)`).
        let child_constraints = BoxConstraints {
            min_width: self.width_factor.map(|f| constraints.max_width * f).unwrap_or(constraints.min_width),
            max_width: self.width_factor.map(|f| constraints.max_width * f).unwrap_or(constraints.max_width),
            min_height: self.height_factor.map(|f| constraints.max_height * f).unwrap_or(constraints.min_height),
            max_height: self.height_factor.map(|f| constraints.max_height * f).unwrap_or(constraints.max_height),
        }
        .enforce(constraints);
        let child_size = cx.layout_child(child, child_constraints);
        // The box itself fills the incoming constraints; the child is positioned
        // inside it per the alignment (Flutter: `size = constraints.constrainSize`).
        let size = constraints.constrain(constraints.biggest());
        let dw = size.width - child_size.width;
        let dh = size.height - child_size.height;
        self.position = Offset::new(
            dw * (self.alignment.x + 1.0) / 2.0,
            dh * (self.alignment.y + 1.0) / 2.0,
        );
        cx.set_child_offset(child, Offset::ZERO);
        size
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        if let Some(child) = cx.children().first().copied() {
            cx.paint_child(child, offset);
        }
    }

    fn transform(&self, _size: Size) -> Option<Affine> {
        let pos = self.position;
        if pos == Offset::ZERO {
            None
        } else {
            Some(Affine::translate((pos.x, pos.y)))
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderFractionallySizedBox"
    }
}
