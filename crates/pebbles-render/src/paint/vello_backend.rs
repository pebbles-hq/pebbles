//! Backend A — full Vello (GPU compute). The [`Painter`] verbs forward straight to a
//! `vello::Scene`. Selected by the `vello` feature; the shared geometry/brush vocabulary
//! ([`Affine`], [`Fill`], …) is re-exported by the parent [`super`] (the paint seam).

use super::{peniko, Affine, Brush, BrushRef, Color, Fill, FontData, Rect, Shape, Stroke};

/// A shaped glyph + font variation coordinate, straight from vello.
pub use vello::{Glyph, NormalizedCoord};

/// A retained drawing buffer — a `vello::Scene`.
pub type Scene = vello::Scene;

/// Create an empty [`Scene`].
pub fn scene() -> Scene {
    Scene::new()
}

/// The drawing surface RenderObjects paint into — a thin wrapper around a
/// mutable [`Scene`] that forwards each verb to `vello::Scene`.
#[repr(transparent)]
pub struct Painter<'a> {
    scene: &'a mut Scene,
}

impl<'a> Painter<'a> {
    /// Wrap a mutable scene as a painter.
    pub fn new(scene: &'a mut Scene) -> Self {
        Painter { scene }
    }

    /// Reborrow as a shorter-lived painter (for sub-scenes / recursion).
    pub fn reborrow(&mut self) -> Painter<'_> {
        Painter { scene: self.scene }
    }

    /// The underlying scene (for the shell's final compose + fragment plumbing).
    pub fn scene_mut(&mut self) -> &mut Scene {
        self.scene
    }

    /// Clear all encoded content (reuse the buffer for a fresh frame/fragment).
    pub fn reset(&mut self) {
        self.scene.reset();
    }

    /// Fill `shape` with `brush`.
    pub fn fill<'b>(
        &mut self,
        style: Fill,
        transform: Affine,
        brush: impl Into<BrushRef<'b>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        self.scene.fill(style, transform, brush, brush_transform, shape);
    }

    /// Stroke `shape` with `brush`.
    pub fn stroke<'b>(
        &mut self,
        style: &Stroke,
        transform: Affine,
        brush: impl Into<BrushRef<'b>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        self.scene.stroke(style, transform, brush, brush_transform, shape);
    }

    /// Push a clip/blend layer (`clip_style` picks fill-rule vs. stroked-outline clip).
    pub fn push_layer<'s>(
        &mut self,
        clip_style: impl Into<peniko::StyleRef<'s>>,
        blend: impl Into<peniko::BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        self.scene.push_layer(clip_style, blend, alpha, transform, clip);
    }

    /// Pop the most recent layer.
    pub fn pop_layer(&mut self) {
        self.scene.pop_layer();
    }

    /// Draw an image (already positioned by `transform`).
    pub fn draw_image<'b>(
        &mut self,
        image: impl Into<peniko::ImageBrushRef<'b>>,
        transform: Affine,
    ) {
        self.scene.draw_image(image, transform);
    }

    /// A GPU-accelerated blurred rounded rectangle (drop shadow).
    pub fn draw_blurred_rounded_rect(
        &mut self,
        transform: Affine,
        rect: Rect,
        brush: Color,
        radius: f64,
        std_dev: f64,
    ) {
        self.scene.draw_blurred_rounded_rect(transform, rect, brush, radius, std_dev);
    }

    /// Append a retained [`Scene`] fragment, translated by `transform`.
    pub fn append(&mut self, fragment: &Scene, transform: Option<Affine>) {
        self.scene.append(fragment, transform);
    }

    /// Draw a shaped glyph run. Wraps vello's fluent builder in one call so the
    /// text object stays backend-agnostic.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_glyphs(
        &mut self,
        font: &FontData,
        font_size: f32,
        normalized_coords: &[NormalizedCoord],
        brush: &Brush,
        transform: Affine,
        glyph_transform: Option<Affine>,
        style: Fill,
        glyphs: impl Iterator<Item = Glyph>,
    ) {
        self.scene
            .draw_glyphs(font)
            .brush(brush)
            .transform(transform)
            .glyph_transform(glyph_transform)
            .font_size(font_size)
            .normalized_coords(normalized_coords)
            .draw(style, glyphs);
    }
}
