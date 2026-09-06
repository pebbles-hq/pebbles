//! More compositing effects: [`RenderClipOval`], [`RenderClipPath`],
//! [`RenderColorFilter`], and [`RenderShaderMask`]. Each wraps a single child and
//! pushes a vello layer around (or over) it — the same shape as [`RenderClipRRect`],
//! with a different clip shape or blend.

use std::rc::Rc;

use pebbles_foundation::{Color, Offset, Rect, Size};
use kurbo::{Affine, BezPath, Ellipse};
use peniko::{BlendMode, Compose, Fill, Mix};

use super::decorated::gradient_brush;
use crate::constraints::BoxConstraints;
use crate::decoration::Gradient;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

fn layout_single_child(cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
    match cx.children().first().copied() {
        Some(child) => {
            let size = cx.layout_child(child, constraints);
            cx.set_child_offset(child, Offset::ZERO);
            size
        }
        None => constraints.constrain(Size::ZERO),
    }
}

// ---------------------------------------------------------------------------
// RenderClipOval — clip the child to the ellipse inscribed in its bounds
// ---------------------------------------------------------------------------

/// Clips its child to the ellipse (a circle for a square box) inscribed in its bounds.
pub struct RenderClipOval;

impl RenderObject for RenderClipOval {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        layout_single_child(cx, constraints)
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let Some(child) = cx.children().first().copied() else { return };
        let bounds = Rect::from_origin_size(offset.to_point(), cx.size());
        let ellipse = Ellipse::new(bounds.center(), (bounds.width() / 2.0, bounds.height() / 2.0), 0.0);
        cx.scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &ellipse);
        cx.paint_child_clipped(child, offset + cx.child_offset(child), bounds);
        cx.scene.pop_layer();
    }

    fn clips_children(&self) -> bool {
        true
    }

    fn debug_name(&self) -> &'static str {
        "RenderClipOval"
    }
}

// ---------------------------------------------------------------------------
// RenderClipPath — clip the child to a caller-supplied path
// ---------------------------------------------------------------------------

/// Clips its child to a path built by a delegate from the box size (Flutter's
/// `ClipPath` + `CustomClipper<Path>`).
pub struct RenderClipPath {
    pub path_fn: Rc<dyn Fn(Size) -> BezPath>,
}

impl RenderObject for RenderClipPath {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        layout_single_child(cx, constraints)
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let Some(child) = cx.children().first().copied() else { return };
        let size = cx.size();
        let bounds = Rect::from_origin_size(offset.to_point(), size);
        // The delegate builds the path in local space; offset it to the box origin.
        let path = (self.path_fn)(size);
        let placed = Affine::translate((offset.x, offset.y)) * &path;
        cx.scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &placed);
        cx.paint_child_clipped(child, offset + cx.child_offset(child), bounds);
        cx.scene.pop_layer();
    }

    fn clips_children(&self) -> bool {
        true
    }

    fn debug_name(&self) -> &'static str {
        "RenderClipPath"
    }
}

// ---------------------------------------------------------------------------
// RenderColorFilter — blend a color over the child
// ---------------------------------------------------------------------------

/// Paints the child, then blends `color` over it with `blend` (Flutter's
/// `ColorFiltered` with a `ColorFilter.mode`).
pub struct RenderColorFilter {
    pub color: Color,
    pub blend: Mix,
}

impl RenderObject for RenderColorFilter {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        layout_single_child(cx, constraints)
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let Some(child) = cx.children().first().copied() else { return };
        let bounds = Rect::from_origin_size(offset.to_point(), cx.size());
        cx.paint_child(child, offset + cx.child_offset(child));
        // A layer whose content (the color) blends with the child beneath it.
        cx.scene.push_layer(Fill::NonZero, self.blend, 1.0, Affine::IDENTITY, &bounds);
        cx.scene.fill(Fill::NonZero, Affine::IDENTITY, self.color, None, &bounds);
        cx.scene.pop_layer();
    }

    fn debug_name(&self) -> &'static str {
        "RenderColorFilter"
    }
}

// ---------------------------------------------------------------------------
// RenderShaderMask — mask the child by a gradient's luminance
// ---------------------------------------------------------------------------

/// Paints the child, then masks it by a gradient's luminance — bright gradient areas
/// keep the child, dark areas hide it (Flutter's `ShaderMask`, the common fade/vignette).
pub struct RenderShaderMask {
    pub gradient: Gradient,
}

impl RenderObject for RenderShaderMask {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        layout_single_child(cx, constraints)
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let Some(child) = cx.children().first().copied() else { return };
        let bounds = Rect::from_origin_size(offset.to_point(), cx.size());
        // Isolate the child in its own layer so the mask multiplies ONLY the child's
        // alpha, not whatever was painted behind it.
        cx.scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &bounds);
        cx.paint_child_clipped(child, offset + cx.child_offset(child), bounds);
        // Multiply the isolated child's alpha by the gradient's luminance. We express
        // this with the `DestIn` compositing operator against a derived gradient whose
        // per-stop ALPHA equals the stop's luminance: `DestIn` keeps the destination
        // (child) weighted by source alpha, so child.alpha *= luminance. This is exactly
        // a luminance mask — luminance is linear in RGB, so masking with the per-stop
        // luminance is identical to sampling the luminance of the blended gradient — and
        // it uses only the isolated-layer + `DestIn` verbs BOTH backends support (the
        // hybrid backend has no dedicated mask-layer primitive).
        let mask = luminance_gradient(&self.gradient, bounds);
        cx.scene.push_layer(
            Fill::NonZero,
            BlendMode::new(Mix::Normal, Compose::DestIn),
            1.0,
            Affine::IDENTITY,
            &bounds,
        );
        cx.scene.fill(Fill::NonZero, Affine::IDENTITY, &mask, None, &bounds);
        cx.scene.pop_layer();
        cx.scene.pop_layer();
    }

    fn debug_name(&self) -> &'static str {
        "RenderShaderMask"
    }
}

/// Build the mask gradient for [`RenderShaderMask`]: the same geometry as the source
/// gradient, but every stop recolored to opaque-white-with-`alpha = luminance` so that
/// compositing it with `DestIn` scales the child's alpha by luminance. Because luminance
/// (`0.2126 R + 0.7152 G + 0.0722 B`) is linear in the color channels — and gradient
/// stops interpolate linearly in those same channels — the per-stop derivation is exact:
/// `luminance(lerp(a, b)) == lerp(luminance(a), luminance(b))`.
fn luminance_gradient(g: &Gradient, rect: Rect) -> peniko::Gradient {
    fn to_luminance_alpha(c: &Color) -> Color {
        let [r, g, b, a] = c.components;
        let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        // RGB is irrelevant under `DestIn` (only source alpha weights the destination);
        // keep it white so the stop is a pure alpha ramp.
        Color::new([1.0, 1.0, 1.0, lum * a])
    }
    let derive = |colors: &[Color]| colors.iter().map(to_luminance_alpha).collect::<Vec<_>>();
    let masked = match g {
        Gradient::Linear { begin, end, colors } => Gradient::Linear {
            begin: *begin,
            end: *end,
            colors: derive(colors),
        },
        Gradient::Radial { center, radius, colors } => Gradient::Radial {
            center: *center,
            radius: *radius,
            colors: derive(colors),
        },
        Gradient::Sweep { center, start_angle, end_angle, colors } => Gradient::Sweep {
            center: *center,
            start_angle: *start_angle,
            end_angle: *end_angle,
            colors: derive(colors),
        },
    };
    gradient_brush(&masked, rect)
}
