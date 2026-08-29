//! [`RenderDecoratedBox`] — paints a [`BoxDecoration`] (shadows, background,
//! border, rounded corners) behind and around its child.

use pebbles_foundation::{Offset, Rect, Size};
use vello::kurbo::{Affine, Shape, Stroke};
use vello::peniko::Fill;

use crate::constraints::BoxConstraints;
use crate::decoration::BoxDecoration;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// Paints a decoration around its (optional) single child.
pub struct RenderDecoratedBox {
    pub decoration: BoxDecoration,
}

impl RenderDecoratedBox {
    pub fn new(decoration: BoxDecoration) -> Self {
        RenderDecoratedBox { decoration }
    }
}

impl RenderObject for RenderDecoratedBox {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        match cx.children().first().copied() {
            Some(child) => {
                let size = cx.layout_child(child, constraints);
                cx.set_child_offset(child, Offset::ZERO);
                size
            }
            None => constraints.constrain(constraints.biggest()),
        }
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        let rect = Rect::from_origin_size(offset.to_point(), cx.size());
        let radii = self.decoration.radius.to_radii();
        let rounded = rect.to_rounded_rect(radii);

        // 1. Shadows (behind everything).
        for shadow in &self.decoration.shadows {
            let shadow_rect =
                Rect::from_origin_size((offset + shadow.offset).to_point(), cx.size())
                    .inflate(shadow.spread, shadow.spread);
            cx.scene.draw_blurred_rounded_rect(
                Affine::IDENTITY,
                shadow_rect,
                shadow.color,
                self.decoration.radius.max(),
                shadow.blur.max(0.01),
            );
        }

        // 2. Background fill.
        if let Some(color) = self.decoration.color {
            cx.scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &rounded);
        }

        // 3. Child, painted on top of the background.
        if let Some(child) = cx.children().first().copied() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }

        // 4. Border, stroked on top (centered on the rounded-rect path).
        if let Some(border) = self.decoration.border {
            let stroke = Stroke::new(border.width);
            cx.scene.stroke(&stroke, Affine::IDENTITY, border.color, None, &rounded.to_path(0.1));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderDecoratedBox"
    }
}
