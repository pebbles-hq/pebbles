//! Prepare: walk the shared display list into GPU-ready geometry on the CPU (no device).

use kurbo::Affine;

use pebbles_render::paint::Scene;
use pebbles_render::paint::display_list::Op;

use crate::gpu_types::SolidVertex;

mod clip;
mod color;
mod glyphs;
mod images;
mod plan;
mod shadows;
mod stroke;
mod tessellate;

pub(crate) use clip::Scissor;
pub(crate) use images::image_uploads;
pub(crate) use plan::{Batch, GlyphRun, ImageQuad, Mesh, SolidMesh};

/// Path-flattening tolerance for recorded shapes (logical px) — a local copy, because
/// `display_list::TOL` is `pub(crate)` to pebbles-render.
pub(crate) const TOL: f64 = 0.1;

/// CPU tessellation context: accumulates flat vertex/index buffers and partitions
/// them into draw batches keyed by the current scissor rect (rect clips from
/// `push_layer`). Non-rect clips and blend/opacity layers are ignored for now —
/// Phase 4 handles the general case.
pub(crate) struct Tess {
    pub(crate) vertices: Vec<SolidVertex>,
    pub(crate) indices: Vec<u32>,
    pub(crate) batch_starts: Vec<(u32, Option<Scissor>)>,
    pub(crate) scissor: Option<Scissor>,
    pub(crate) clip_stack: Vec<Option<Scissor>>,
    pub(crate) glyphs: Vec<GlyphRun>,
    pub(crate) images: Vec<ImageQuad>,
    pub(crate) shadows: Vec<crate::gpu_types::ShadowVertex>,
    pub(crate) img_idx: u32,
}

impl Tess {
    fn new() -> Self {
        Self {
            vertices: Vec::new(),
            indices: Vec::new(),
            batch_starts: Vec::new(),
            scissor: None,
            clip_stack: Vec::new(),
            glyphs: Vec::new(),
            images: Vec::new(),
            shadows: Vec::new(),
            img_idx: 0,
        }
    }

    fn finish(self) -> Mesh {
        let mut batches = Vec::with_capacity(self.batch_starts.len());
        for (i, (start, scissor)) in self.batch_starts.iter().enumerate() {
            let end = self.batch_starts.get(i + 1).map(|(s, _)| *s).unwrap_or(self.indices.len() as u32);
            batches.push(Batch { range: *start..end, scissor: *scissor });
        }
        Mesh {
            solid: SolidMesh { vertices: self.vertices, indices: self.indices, batches },
            glyphs: self.glyphs,
            images: self.images,
            shadows: self.shadows,
        }
    }
}

/// Tessellate the Phase-1 solid subset of `scene` into GPU-ready triangles, batched
/// by scissor rect, plus the Phase-2 glyph runs, Phase-3 image quads and shadow
/// quads. Gradients and non-rect clips remain in Phase 4.
pub(crate) fn mesh(scene: &Scene) -> Mesh {
    let mut tess = Tess::new();
    emit(scene.ops(), Affine::IDENTITY, &mut tess);
    tess.finish()
}

fn emit(ops: &[Op], outer: Affine, tess: &mut Tess) {
    for op in ops {
        match op {
            Op::Fill { style, transform, brush: peniko::Brush::Solid(color), shape, .. } => {
                let path = shape.to_path(TOL);
                tess.fill(&path, outer * *transform, *style, *color);
            }
            Op::Stroke { style, transform, brush: peniko::Brush::Solid(color), shape, .. } => {
                let path = shape.to_path(TOL);
                tess.stroke(&path, outer * *transform, style, *color);
            }
            Op::PushLayer { blend, alpha, transform, clip, .. } => {
                let scissor = if *blend == peniko::BlendMode::default() && *alpha >= 1.0 {
                    let clip_path = clip.to_path(TOL);
                    clip::scissor_rect(&clip_path, outer * *transform)
                } else {
                    None
                };
                tess.push_clip(scissor);
            }
            Op::PopLayer => tess.pop_clip(),
            Op::Glyphs {
                font, font_size, font_weight, brush, transform, glyph_transform, glyphs, ..
            } => {
                tess.glyph_run(
                    font,
                    *font_size,
                    *font_weight,
                    brush,
                    outer * *transform,
                    *glyph_transform,
                    glyphs,
                );
            }
            Op::Fragment { transform, ops, .. } => {
                emit(ops, outer * transform.unwrap_or(Affine::IDENTITY), tess);
            }
            Op::Image { image, transform, .. } if image.image.width != 0 && image.image.height != 0 => {
                tess.image(image, outer * *transform, tess.img_idx);
                tess.img_idx += 1;
            }
            Op::BlurredRect { transform, rect, brush, radius, std_dev, .. } => {
                tess.shadow(rect, outer * *transform, *radius, *std_dev, *brush);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pebbles_render::paint::{Painter, Stroke};
    use peniko::Color;

    #[test]
    fn solid_fill_tessellates_to_a_single_unclipped_batch() {
        let mut scene = Scene::new();
        Painter::new(&mut scene).fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(255, 0, 0, 255),
            None,
            &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
        );

        let mesh = mesh(&scene).solid;
        assert!(!mesh.is_empty());
        assert_eq!(mesh.batches.len(), 1);
        assert_eq!(mesh.batches[0].scissor, None);
        assert!(mesh.batches[0].range.end > mesh.batches[0].range.start);
    }

    #[test]
    fn rect_clip_partitions_geometry_into_scissored_batches() {
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        p.push_layer(
            peniko::Fill::NonZero,
            peniko::BlendMode::default(),
            1.0,
            Affine::IDENTITY,
            &kurbo::Rect::new(0.0, 0.0, 100.0, 50.0),
        );
        p.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(0, 255, 0, 255),
            None,
            &kurbo::Rect::new(0.0, 0.0, 50.0, 25.0),
        );
        p.pop_layer();
        p.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(0, 0, 255, 255),
            None,
            &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
        );

        let mesh = mesh(&scene).solid;
        assert_eq!(mesh.batches.len(), 2);
        assert_eq!(mesh.batches[1].scissor, None);
        let scissor = mesh.batches[0].scissor.expect("clipped batch has a scissor");
        assert_eq!((scissor.x, scissor.y, scissor.width, scissor.height), (0, 0, 100, 50));
    }

    #[test]
    fn rotated_clip_is_not_scissored() {
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        p.push_layer(
            peniko::Fill::NonZero,
            peniko::BlendMode::default(),
            1.0,
            Affine::rotate(std::f64::consts::FRAC_PI_4),
            &kurbo::Rect::new(0.0, 0.0, 100.0, 50.0),
        );
        p.fill(
            peniko::Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(0, 255, 0, 255),
            None,
            &kurbo::Rect::new(0.0, 0.0, 50.0, 25.0),
        );
        p.pop_layer();

        let mesh = mesh(&scene).solid;
        assert_eq!(mesh.batches.len(), 1);
        assert_eq!(mesh.batches[0].scissor, None);
    }

    #[test]
    fn dashed_stroke_tessellates_geometry() {
        let mut scene = Scene::new();
        let style = Stroke::new(2.0).with_dashes(0.0, [6.0, 3.0]);
        Painter::new(&mut scene).stroke(
            &style,
            Affine::IDENTITY,
            Color::from_rgba8(0, 0, 0, 255),
            None,
            &kurbo::Rect::new(0.0, 0.0, 100.0, 40.0),
        );

        let mesh = mesh(&scene).solid;
        assert!(!mesh.is_empty());
        assert!(mesh.vertices.len() > 4);
    }
}
