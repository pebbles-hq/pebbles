//! The prepare-stage plan types: the geometry arenas and the draw batching.

use lyon_tessellation::VertexBuffers;

use pebbles_render::paint::Glyph;
use peniko::FontData;

use super::Tess;
use super::clip::Scissor;
use crate::gpu_types::{ImageVertex, ShadowVertex, SolidVertex};

/// One draw call: an index range into [`SolidMesh::indices`] plus the scissor rect
/// (if any) active while drawing it.
#[derive(Clone, Debug)]
pub(crate) struct Batch {
    pub range: std::ops::Range<u32>,
    pub scissor: Option<Scissor>,
}

pub(crate) struct SolidMesh {
    pub vertices: Vec<SolidVertex>,
    pub indices: Vec<u32>,
    pub batches: Vec<Batch>,
}

impl SolidMesh {
    pub(crate) fn is_empty(&self) -> bool {
        self.batches.is_empty()
    }
}

/// A shaped glyph run, transformed to physical-pixel space, ready for the host's
/// glyph atlas.
pub(crate) struct GlyphRun {
    pub font: FontData,
    pub font_size: f32,
    pub font_weight: u16,
    pub color: [f32; 4],
    pub glyphs: Vec<Glyph>,
    pub scissor: Option<Scissor>,
    pub synthesize_italic: bool,
}

/// Everything the renderer needs to draw one frame.
pub(crate) struct Mesh {
    pub solid: SolidMesh,
    pub glyphs: Vec<GlyphRun>,
    pub images: Vec<ImageQuad>,
    pub shadows: Vec<ShadowVertex>,
}

/// One image draw: the image index (into [`super::images::image_uploads`]) + a quad in
/// physical-pixel space.
pub(crate) struct ImageQuad {
    pub image_index: u32,
    pub vertices: [ImageVertex; 4],
    pub scissor: Option<Scissor>,
}

impl Tess {
    pub(crate) fn append(&mut self, geometry: VertexBuffers<SolidVertex, u32>) {
        if geometry.indices.is_empty() {
            return;
        }
        if self.batch_starts.last().is_none_or(|(_, s)| *s != self.scissor) {
            self.batch_starts.push((self.indices.len() as u32, self.scissor));
        }
        let base = self.vertices.len() as u32;
        self.vertices.extend(geometry.vertices);
        self.indices.extend(geometry.indices.into_iter().map(|index| base + index));
    }
}
