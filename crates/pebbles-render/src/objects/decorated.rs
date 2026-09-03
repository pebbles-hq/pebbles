//! [`RenderDecoratedBox`] — paints a [`BoxDecoration`] (shadows, background,
//! border, rounded corners) behind and around its child.

use pebbles_foundation::{Alignment, Axis, Offset, Rect, Size};
use vello::kurbo::{Affine, BezPath, Circle, Point, Shape, Stroke};
use vello::peniko::Fill;

use crate::constraints::BoxConstraints;
use crate::decoration::{BlendMode, BorderSide, BoxDecoration, BoxShape, Gradient, ImageFit};
use crate::object::RenderObject;
use crate::tree::{IntrinsicCx, LayoutCx, PaintCx};

/// Paints a decoration around its (optional) single child.
pub struct RenderDecoratedBox {
    pub decoration: BoxDecoration,
    /// Painted AFTER the child (Flutter's `foregroundDecoration`) — e.g. an inner
    /// border or overlay drawn on top of the content.
    pub foreground: Option<BoxDecoration>,
}

impl RenderDecoratedBox {
    pub fn new(decoration: BoxDecoration) -> Self {
        RenderDecoratedBox { decoration, foreground: None }
    }
}

impl RenderObject for RenderDecoratedBox {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        match cx.children().first().copied() {
            Some(child) => {
                let size = cx.layout_child(child, constraints);
                cx.set_child_offset(child, Offset::ZERO);
                size
            }
            None => constraints.constrain(constraints.biggest()),
        }
    }

    fn intrinsic(&self, cx: &mut IntrinsicCx<'_>, axis: Axis, cross_extent: f64) -> Option<f64> {
        // Decoration paints around the child, not beside it — pass through.
        cx.children()
            .first()
            .copied()
            .and_then(|child| cx.child_intrinsic(child, axis, cross_extent))
    }

    fn baseline(&self, cx: &mut LayoutCx<'_>) -> Option<f64> {
        cx.children().first().copied().and_then(|child| cx.child_baseline(child))
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let size = cx.size();
        let rect = Rect::from_origin_size(offset.to_point(), size);
        let d = &self.decoration;

        // A box that hasn't been laid out yet (or collapsed to nothing) has a
        // zero/negative size. Painting a shadow/blur or image for it makes vello
        // allocate a 0-sized GPU texture, which wgpu rejects as an invalid
        // texture — poisoning the device and (with the old blocking present)
        // freezing the window to black. Nothing is visible at this size anyway,
        // so skip the GPU-texture-producing draws entirely.
        if size.width < 0.5 || size.height < 0.5 {
            // Still paint the child (it may size itself); just no bg/border/shadow.
            if let Some(child) = cx.children().first().copied() {
                cx.paint_child(child, offset + cx.child_offset(child));
            }
            return;
        }

        // The outline path, plus an equivalent corner radius for the shadow.
        let (path, shadow_radius) = outline(d, size, rect);

        // 1. Shadows (behind everything). Guard each blurred rect against a
        // degenerate (≤0) size after spread — a negative spread can collapse it.
        for shadow in &d.shadows {
            let shadow_rect = Rect::from_origin_size((offset + shadow.offset).to_point(), size)
                .inflate(shadow.spread, shadow.spread);
            if shadow_rect.width() < 0.5 || shadow_rect.height() < 0.5 {
                continue;
            }
            if pebbles_foundation::log::dev_mode() {
                pebbles_foundation::log::trace(
                    pebbles_foundation::log::Cat::Gpu,
                    format!("paint shadow blur {:.0}×{:.0} r={:.1}", shadow_rect.width(), shadow_rect.height(), shadow.blur),
                );
            }
            cx.scene.draw_blurred_rounded_rect(
                Affine::IDENTITY,
                shadow_rect,
                shadow.color,
                shadow_radius,
                shadow.blur.max(0.01),
            );
        }

        // 2. Background fill + image.
        fill_surface(cx, rect, size, &path, d);

        // 3. Child, painted on top of the background.
        if let Some(child) = cx.children().first().copied() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }

        // 4. Border. A uniform border strokes the outline crisply; a per-side border
        // strokes each edge as a straight inset line.
        border_surface(cx, rect, &path, d);

        // 5. Foreground decoration — painted over the child and border.
        if let Some(fg) = &self.foreground {
            let (fg_path, _) = outline(fg, size, rect);
            fill_surface(cx, rect, size, &fg_path, fg);
            border_surface(cx, rect, &fg_path, fg);
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderDecoratedBox"
    }
}

/// The outline path for a decoration, plus an equivalent corner radius for shadows.
fn outline(d: &BoxDecoration, size: Size, rect: Rect) -> (BezPath, f64) {
    match d.shape {
        BoxShape::Circle => {
            let r = size.width.min(size.height) / 2.0;
            let center = Point::new(rect.x0 + size.width / 2.0, rect.y0 + size.height / 2.0);
            (Circle::new(center, r).to_path(0.1), r)
        }
        BoxShape::Rectangle => {
            (rect.to_rounded_rect(d.radius.to_radii()).to_path(0.1), d.radius.max())
        }
    }
}

/// Paint a decoration's background fill + image (no shadow, no border).
fn fill_surface(cx: &mut PaintCx<'_>, rect: Rect, size: Size, path: &BezPath, d: &BoxDecoration) {
    let has_fill = d.gradient.is_some() || d.color.is_some();
    if has_fill {
        let layered = d.blend.is_some();
        if let Some(blend) = d.blend {
            cx.scene.push_layer(Fill::NonZero, blend, 1.0, Affine::IDENTITY, path);
        }
        if let Some(gradient) = &d.gradient {
            if pebbles_foundation::log::dev_mode() {
                pebbles_foundation::log::trace(
                    pebbles_foundation::log::Cat::Gpu,
                    format!("paint gradient fill {:.0}×{:.0} @ {:.0},{:.0}", size.width, size.height, rect.x0, rect.y0),
                );
            }
            let brush = gradient_brush(gradient, rect);
            cx.scene.fill(Fill::NonZero, Affine::IDENTITY, &brush, None, path);
        } else if let Some(color) = d.color {
            cx.scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, path);
        }
        if layered {
            cx.scene.pop_layer();
        }
    }
    if let Some(image) = &d.image {
        let iw = image.image.width as f64;
        let ih = image.image.height as f64;
        if iw > 0.0 && ih > 0.0 {
            let (sx, sy) = match d.image_fit {
                ImageFit::Cover => {
                    let s = (size.width / iw).max(size.height / ih);
                    (s, s)
                }
                ImageFit::Contain => {
                    let s = (size.width / iw).min(size.height / ih);
                    (s, s)
                }
                ImageFit::Fill => (size.width / iw, size.height / ih),
                ImageFit::None => (1.0, 1.0),
            };
            let (dw, dh) = (iw * sx, ih * sy);
            let tx = rect.x0 + (size.width - dw) / 2.0;
            let ty = rect.y0 + (size.height - dh) / 2.0;
            let placement = Affine::translate((tx, ty)) * Affine::scale_non_uniform(sx, sy);
            if pebbles_foundation::log::dev_mode() {
                pebbles_foundation::log::trace(
                    pebbles_foundation::log::Cat::Gpu,
                    format!("paint image {}×{} (src)", image.image.width, image.image.height),
                );
            }
            cx.scene.push_layer(Fill::NonZero, BlendMode::Normal, 1.0, Affine::IDENTITY, path);
            cx.scene.draw_image(image, placement);
            cx.scene.pop_layer();
        }
    }
}

/// Stroke a decoration's border (uniform strokes the outline; per-side strokes insets).
fn border_surface(cx: &mut PaintCx<'_>, rect: Rect, path: &BezPath, d: &BoxDecoration) {
    if let Some(border) = d.border {
        if border.is_uniform() {
            let side = border.top;
            if side.width > 0.0 {
                cx.scene.stroke(&Stroke::new(side.width), Affine::IDENTITY, side.color, None, path);
            }
        } else {
            paint_side(cx, border.top, (rect.x0, rect.y0 + border.top.width / 2.0), (rect.x1, rect.y0 + border.top.width / 2.0));
            paint_side(cx, border.bottom, (rect.x0, rect.y1 - border.bottom.width / 2.0), (rect.x1, rect.y1 - border.bottom.width / 2.0));
            paint_side(cx, border.left, (rect.x0 + border.left.width / 2.0, rect.y0), (rect.x0 + border.left.width / 2.0, rect.y1));
            paint_side(cx, border.right, (rect.x1 - border.right.width / 2.0, rect.y0), (rect.x1 - border.right.width / 2.0, rect.y1));
        }
    }
}

/// Map an alignment (`-1..1` in each axis) to an absolute point within `rect`.
fn point_in(rect: Rect, a: Alignment) -> Point {
    Point::new(
        rect.x0 + (a.x + 1.0) / 2.0 * rect.width(),
        rect.y0 + (a.y + 1.0) / 2.0 * rect.height(),
    )
}

/// Build a peniko gradient positioned within `rect` from a [`Gradient`] spec.
fn gradient_brush(g: &Gradient, rect: Rect) -> peniko::Gradient {
    use vello::peniko::Gradient as PGrad;
    match g {
        Gradient::Linear { begin, end, colors } => {
            PGrad::new_linear(point_in(rect, *begin), point_in(rect, *end)).with_stops(&colors[..])
        }
        Gradient::Radial { center, radius, colors } => {
            let r = (rect.width().min(rect.height()) * radius) as f32;
            PGrad::new_radial(point_in(rect, *center), r).with_stops(&colors[..])
        }
        Gradient::Sweep { center, start_angle, end_angle, colors } => {
            // `center` offsets the pivot within the box (`-1..1` per axis); angles
            // are radians clockwise from the positive X axis.
            let pivot = point_in(rect, *center);
            PGrad::new_sweep(pivot, *start_angle as f32, *end_angle as f32)
                .with_stops(&colors[..])
        }
    }
}

/// Stroke a single border edge as a straight line from `p0` to `p1`.
fn paint_side(cx: &mut PaintCx<'_>, side: BorderSide, p0: (f64, f64), p1: (f64, f64)) {
    if side.width <= 0.0 {
        return;
    }
    let mut line = BezPath::new();
    line.move_to(p0);
    line.line_to(p1);
    cx.scene.stroke(&Stroke::new(side.width), Affine::IDENTITY, side.color, None, &line);
}
