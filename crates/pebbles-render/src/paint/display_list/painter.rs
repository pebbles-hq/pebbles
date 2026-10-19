//! The [`Painter`]: the recording API RenderObjects speak. Every verb records an
//! [`Op`](super::op::Op) onto a [`Scene`](super::Scene).

use std::sync::Arc;

use kurbo::{Affine, Rect, Shape, Stroke};
use peniko::{Brush, BrushRef, Color, Fill, FontData};

use super::Scene;
use super::op::{Geometry, Glyph, LayerEffect, NormalizedCoord, Op, PathHint, PixelSnap};

/// The fill-rule for a recorded layer's clip path. Pebbles only ever clips with a fill
/// rule (never a stroked-outline clip), and `vello_hybrid`'s `push_layer` clip is a filled
/// path regardless — so a stroked clip-style degrades to `NonZero` at record time.
fn clip_fill_rule(s: peniko::StyleRef<'_>) -> Fill {
    match s {
        peniko::StyleRef::Fill(f) => f,
        peniko::StyleRef::Stroke(_) => Fill::NonZero,
    }
}

/// The drawing surface RenderObjects paint into — records each verb as an `Op` on the
/// retained list. Signatures mirror the old vello backend exactly.
pub struct Painter<'a> {
    scene: &'a mut Scene,
    hint: PathHint,
    snap: PixelSnap,
}

impl<'a> Painter<'a> {
    /// Wrap a mutable scene as a painter.
    pub fn new(scene: &'a mut Scene) -> Self {
        Painter { scene, hint: PathHint::Auto, snap: PixelSnap::Auto }
    }

    /// Reborrow as a shorter-lived painter (for sub-scenes / recursion).
    pub fn reborrow(&mut self) -> Painter<'_> {
        Painter { scene: self.scene, hint: self.hint, snap: self.snap }
    }

    /// The underlying scene (for the shell's flush + fragment plumbing).
    pub fn scene_mut(&mut self) -> &mut Scene {
        self.scene
    }

    /// Set the [`PathHint`] recorded on every following fill, stroke and clip layer until changed.
    pub fn set_path_hint(&mut self, hint: PathHint) {
        self.hint = hint;
    }

    /// Set the [`PixelSnap`] recorded on every following fill/stroke/image until changed again.
    pub fn set_pixel_snap(&mut self, snap: PixelSnap) {
        self.snap = snap;
    }

    /// Clear all recorded content.
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
        self.scene.push(Op::Fill {
            style,
            transform,
            brush: brush.into().to_owned(),
            brush_transform,
            shape: Geometry::from_shape(shape),
            hint: self.hint,
            snap: self.snap,
        });
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
        self.scene.push(Op::Stroke {
            style: style.clone(),
            transform,
            brush: brush.into().to_owned(),
            brush_transform,
            shape: Geometry::from_shape(shape),
            hint: self.hint,
            snap: self.snap,
        });
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
        self.scene.push(Op::PushLayer {
            clip_fill: clip_fill_rule(clip_style.into()),
            blend: blend.into(),
            alpha,
            transform,
            clip: Geometry::from_shape(clip),
            hint: self.hint,
        });
    }

    /// Push an effect layer (09 §10.1, Flutter `ImageFiltered`): the children render offscreen,
    /// `effect` filters them, and the result composites clipped by `clip` (local, under
    /// `transform`). Close with [`pop_layer`](Self::pop_layer).
    pub fn push_effect_layer(&mut self, effect: LayerEffect, clip: Option<&impl Shape>, transform: Affine) {
        let effect = match effect {
            LayerEffect::Chain(es) => LayerEffect::chain(es.iter().cloned()),
            e => e,
        };
        self.scene.push(Op::PushEffectLayer { effect, clip: clip.map(Geometry::from_shape), transform });
    }

    /// Push a backdrop filter (09 §10.4, Flutter `BackdropFilter`): what is already drawn under
    /// `clip` is filtered by `effect` and drawn back with `blend` at `alpha`; the children then
    /// draw on top, clipped. Close with [`pop_layer`](Self::pop_layer).
    pub fn push_backdrop_filter(
        &mut self,
        effect: LayerEffect,
        clip: &impl Shape,
        transform: Affine,
        blend: impl Into<peniko::BlendMode>,
        alpha: f32,
    ) {
        let effect = match effect {
            LayerEffect::Chain(es) => LayerEffect::chain(es.iter().cloned()),
            e => e,
        };
        self.scene.push(Op::PushBackdropFilter {
            effect,
            clip: Geometry::from_shape(clip),
            transform,
            blend: blend.into(),
            alpha,
        });
    }

    /// Pop the most recent layer.
    pub fn pop_layer(&mut self) {
        self.scene.push(Op::PopLayer);
    }

    /// Draw an image (already positioned by `transform`).
    pub fn draw_image<'b>(&mut self, image: impl Into<peniko::ImageBrushRef<'b>>, transform: Affine) {
        self.scene.push(Op::Image { image: image.into().to_owned(), transform, snap: self.snap });
    }

    /// Fill `shape` with a blurred coverage mask (02 R42); `sigma` in local units.
    pub fn fill_blurred(
        &mut self,
        style: Fill,
        transform: Affine,
        color: Color,
        shape: &impl Shape,
        sigma: f32,
        blur_style: super::op::BlurStyle,
    ) {
        self.scene.push(Op::BlurredFill {
            style,
            transform,
            color,
            shape: Geometry::from_shape(shape),
            sigma,
            blur_style,
        });
    }

    /// Draw a glyph run's union as one blurred mask (02 R42; text shadows).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_glyphs_blurred(
        &mut self,
        font: &FontData,
        font_size: f32,
        normalized_coords: &[NormalizedCoord],
        transform: Affine,
        glyph_transform: Option<Affine>,
        glyphs: impl Iterator<Item = Glyph>,
        color: Color,
        sigma: f32,
    ) {
        self.scene.push(Op::BlurredGlyphs {
            font: font.clone(),
            font_size,
            normalized_coords: normalized_coords.to_vec(),
            transform,
            glyph_transform,
            glyphs: glyphs.collect(),
            color,
            sigma,
        });
    }

    /// Draw a [`DynamicImage`](crate::paint::DynamicImage) (02 R37); never mipmapped.
    pub fn draw_dynamic_image(
        &mut self,
        image: &crate::paint::DynamicImage,
        quality: peniko::ImageQuality,
        alpha: f32,
        transform: Affine,
    ) {
        self.scene.push(Op::DynamicImage { frame: image.frame(), quality, alpha, transform });
    }

    /// Draw `image` nine-sliced into `dst` (02 R36): `center` (image px) is the stretchable part.
    pub fn draw_image_nine(
        &mut self,
        image: &peniko::ImageBrush,
        center: Rect,
        dst: Rect,
        transform: Affine,
    ) {
        self.scene.push(Op::ImageNine { image: image.clone(), center, dst, transform });
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
        self.scene.push(Op::BlurredRect { transform, rect, brush, radius, std_dev });
    }

    /// Append a retained [`Scene`] fragment, translated by `transform`. The fragment's
    /// ops are shared (an `Arc`) so re-appending each frame is O(1) — no deep clone.
    pub fn append(&mut self, fragment: &Scene, transform: Option<Affine>) {
        self.scene.push(Op::Fragment { transform, ops: Arc::clone(&fragment.ops) });
    }

    /// Record a shaped glyph run (owned; the flush resolves glyphs against the hybrid
    /// atlas in `Resources`).
    // Mirrors the vello backend's `draw_glyphs` signature exactly (backend parity); the flat
    // font+size+coords+brush+transforms+style+glyphs list is the call contract, not a smell.
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
        self.scene.push(Op::Glyphs {
            font: font.clone(),
            font_size,
            normalized_coords: normalized_coords.to_vec(),
            brush: brush.clone(),
            transform,
            glyph_transform,
            style,
            glyphs: glyphs.collect(),
        });
    }
}
