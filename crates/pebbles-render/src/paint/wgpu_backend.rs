//! The transition wgpu paint backend. It keeps the backend-neutral retained op-list
//! intact; the shell owns all GPU state and will consume these operations in later
//! renderer phases.

use std::rc::Rc;

use super::{Affine, BezPath, Brush, BrushRef, Color, Fill, FontData, Rect, Shape, Stroke, peniko};
use bytemuck::{Pod, Zeroable};
use lyon::math::point;
use lyon::path::Path;
use lyon::tessellation::{
    BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, FillVertexConstructor, VertexBuffers,
};

/// Path-flattening tolerance for recorded shapes (logical px).
const TOL: f64 = 0.1;
/// Tessellation tolerance for screen-space geometry (physical px).
const TESS_TOL: f32 = 0.1;
/// Stroke-outline expansion tolerance (physical px).
const STROKE_TOL: f64 = 0.1;

#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}

pub type NormalizedCoord = i16;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
}

/// An axis-aligned clip rectangle in physical-pixel space, for `set_scissor_rect`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Scissor {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Scissor {
    fn intersect(self, other: Scissor) -> Scissor {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);
        let x1 = (self.x + self.width).min(other.x + other.width);
        let y1 = (self.y + self.height).min(other.y + other.height);
        if x1 <= x0 || y1 <= y0 {
            Scissor { x: 0, y: 0, width: 0, height: 0 }
        } else {
            Scissor { x: x0, y: y0, width: x1 - x0, height: y1 - y0 }
        }
    }
}

/// One draw call: an index range into [`SolidMesh::indices`] plus the scissor rect
/// (if any) active while drawing it. Geometry sharing a scissor rect is grouped into
/// one batch so the host issues a single scissor + draw per batch.
#[derive(Clone, Debug)]
pub struct Batch {
    pub range: std::ops::Range<u32>,
    pub scissor: Option<Scissor>,
}

pub struct SolidMesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub batches: Vec<Batch>,
}

impl SolidMesh {
    pub fn is_empty(&self) -> bool {
        self.batches.is_empty()
    }
}

/// A shaped glyph run, transformed to physical-pixel space, ready for the host's
/// glyph atlas. `font` carries the raw font bytes (+ face index) so the host can
/// register it and rasterize via cosmic-text's `SwashCache`. The glyphs' `x`/`y`
/// are the pen positions (baseline origin) in physical pixels.
pub struct GlyphRun {
    pub font: FontData,
    pub font_size: f32,
    pub font_weight: u16,
    pub color: [f32; 4],
    pub glyphs: Vec<Glyph>,
    pub scissor: Option<Scissor>,
    /// Synthesized italic (the font has no italic face, so the shaper skews it) —
    /// the host rasterizes with cosmic-text's `FAKE_ITALIC` skew to match.
    pub synthesize_italic: bool,
}

/// Everything the wgpu host needs to draw one frame: the solid triangle mesh, the
/// glyph runs (textured quads), image quads, and shadow quads.
pub struct Mesh {
    pub solid: SolidMesh,
    pub glyphs: Vec<GlyphRun>,
    pub images: Vec<ImageQuad>,
    pub shadows: Vec<ShadowVertex>,
}

/// CPU pixel data for one recorded image, handed to the shell for GPU upload.
pub struct ImageUpload {
    /// Stable identity (source pixel-buffer pointer) for the shell's texture cache,
    /// so the same image isn't re-uploaded every frame.
    pub id: u64,
    pub width: u32,
    pub height: u32,
    /// Unmultiplied RGBA8 (Bgra8 sources are byte-swapped). Row-major, tightly packed.
    pub rgba8: Vec<u8>,
}

/// A textured quad vertex: physical-pixel position + source-image UV.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ImageVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
}

/// One image draw: the image index (into [`Scene::image_uploads`]) + a quad in
/// physical-pixel space (with the image's affine applied to the source rect).
pub struct ImageQuad {
    pub image_index: u32,
    pub vertices: [ImageVertex; 4],
    pub scissor: Option<Scissor>,
}

/// A shadow quad vertex: the inflated quad corner plus the (uninflated) rounded
/// rect + blur parameters, so all shadows share one draw call.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ShadowVertex {
    pub position: [f32; 2],
    pub rect: [f32; 4],
    pub radius: f32,
    pub sigma: f32,
    pub color: [f32; 4],
}

fn clip_fill_rule(style: peniko::StyleRef<'_>) -> Fill {
    match style {
        peniko::StyleRef::Fill(fill) => fill,
        peniko::StyleRef::Stroke(_) => Fill::NonZero,
    }
}

/// The rasterization contract shared by the current hybrid backend and wgpu.
#[derive(Clone)]
#[allow(dead_code)]
enum Op {
    Fill {
        style: Fill,
        transform: Affine,
        brush: Brush,
        brush_transform: Option<Affine>,
        path: BezPath,
    },
    Stroke {
        style: Stroke,
        transform: Affine,
        brush: Brush,
        brush_transform: Option<Affine>,
        path: BezPath,
    },
    PushLayer {
        clip_fill: Fill,
        blend: peniko::BlendMode,
        alpha: f32,
        transform: Affine,
        clip: BezPath,
    },
    PopLayer,
    Image {
        image: peniko::ImageBrush,
        transform: Affine,
    },
    BlurredRect {
        transform: Affine,
        rect: Rect,
        brush: Color,
        radius: f64,
        std_dev: f64,
    },
    Glyphs {
        font: FontData,
        font_size: f32,
        font_weight: u16,
        normalized_coords: Vec<NormalizedCoord>,
        brush: Brush,
        transform: Affine,
        glyph_transform: Option<Affine>,
        style: Fill,
        glyphs: Vec<Glyph>,
    },
    Fragment {
        transform: Option<Affine>,
        ops: Rc<Vec<Op>>,
    },
}

/// A retained, backend-neutral paint command buffer.
#[derive(Default, Clone)]
pub struct Scene {
    ops: Vec<Op>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.ops.clear();
    }

    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    pub fn append(&mut self, fragment: &Scene, transform: Option<Affine>) {
        self.ops.push(Op::Fragment { transform, ops: Rc::new(fragment.ops.clone()) });
    }

    /// Tessellate the Phase-1 solid subset into GPU-ready triangles, batched by
    /// scissor rect, plus the Phase-2 glyph runs, Phase-3 image quads and shadow
    /// quads. Gradients and non-rect clips remain in Phase 4.
    pub fn mesh(&self) -> Mesh {
        let mut tess = Tess::new();
        emit(&self.ops, Affine::IDENTITY, &mut tess);
        tess.finish()
    }

    /// The CPU pixel data for every image op, in the SAME order the mesh visits
    /// them — so `image_uploads()[i]` is the texture the mesh references as image
    /// index `i`. Zero-size images are skipped in BOTH walks so the indices align.
    pub fn image_uploads(&self) -> Vec<ImageUpload> {
        let mut out = Vec::new();
        collect_images(&self.ops, &mut out);
        out
    }
}

fn image_to_upload(img: &peniko::ImageBrush) -> ImageUpload {
    let d = &img.image;
    let bytes: &[u8] = d.data.as_ref();
    let mut rgba8 = bytes.to_vec();
    if matches!(d.format, peniko::ImageFormat::Bgra8) {
        for px in rgba8.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
    }
    ImageUpload { id: bytes.as_ptr() as usize as u64, width: d.width, height: d.height, rgba8 }
}

/// Collect image uploads in mesh-visit order (ops in order, recursing fragments),
/// skipping zero-size images exactly as [`emit`] does.
fn collect_images(ops: &[Op], out: &mut Vec<ImageUpload>) {
    for op in ops {
        match op {
            Op::Image { image, .. } if image.image.width != 0 && image.image.height != 0 => {
                out.push(image_to_upload(image));
            }
            Op::Fragment { ops, .. } => collect_images(ops, out),
            _ => {}
        }
    }
}

/// CPU tessellation context: accumulates flat vertex/index buffers and partitions
/// them into draw batches keyed by the current scissor rect (rect clips from
/// `push_layer`). Non-rect clips and blend/opacity layers are ignored for now —
/// Phase 4 handles the general case.
struct Tess {
    vertices: Vec<Vertex>,
    indices: Vec<u32>,
    batch_starts: Vec<(u32, Option<Scissor>)>,
    scissor: Option<Scissor>,
    clip_stack: Vec<Option<Scissor>>,
    glyphs: Vec<GlyphRun>,
    images: Vec<ImageQuad>,
    shadows: Vec<ShadowVertex>,
    img_idx: u32,
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

    fn fill(&mut self, path: &BezPath, transform: Affine, style: Fill, color: Color) {
        let screen = transformed(path, transform);
        let lyon = to_lyon_path(&screen);
        let mut geometry = VertexBuffers::new();
        let mut tess = FillTessellator::new();
        let options = FillOptions::default().with_tolerance(TESS_TOL).with_fill_rule(match style {
            Fill::EvenOdd => FillRule::EvenOdd,
            Fill::NonZero => FillRule::NonZero,
        });
        let _ = tess.tessellate_path(
            &lyon,
            &options,
            &mut BuffersBuilder::new(&mut geometry, SolidVertex(rgba(color))),
        );
        self.append(geometry);
    }

    fn stroke(&mut self, path: &BezPath, transform: Affine, style: &Stroke, color: Color) {
        let screen = transformed(path, transform);
        // Expand the stroke outline with kurbo (handles join/cap/dash), then fill it.
        let outline = kurbo::stroke(
            screen.elements().iter().copied(),
            style,
            &kurbo::StrokeOpts::default(),
            STROKE_TOL,
        );
        let lyon = to_lyon_path(&outline);
        let mut geometry = VertexBuffers::new();
        let mut tess = FillTessellator::new();
        let options = FillOptions::default().with_tolerance(TESS_TOL).with_fill_rule(FillRule::NonZero);
        let _ = tess.tessellate_path(
            &lyon,
            &options,
            &mut BuffersBuilder::new(&mut geometry, SolidVertex(rgba(color))),
        );
        self.append(geometry);
    }

    fn push_clip(&mut self, scissor: Option<Scissor>) {
        self.clip_stack.push(self.scissor);
        self.scissor = match (self.scissor, scissor) {
            (None, None) => None,
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (Some(a), Some(b)) => Some(a.intersect(b)),
        };
    }

    fn pop_clip(&mut self) {
        self.scissor = self.clip_stack.pop().unwrap_or(None);
    }

    fn append(&mut self, geometry: VertexBuffers<Vertex, u32>) {
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

    #[allow(clippy::too_many_arguments)]
    fn glyph_run(
        &mut self,
        font: &FontData,
        font_size: f32,
        font_weight: u16,
        brush: &Brush,
        transform: Affine,
        glyph_transform: Option<Affine>,
        glyphs: &[Glyph],
    ) {
        let color = match brush {
            Brush::Solid(c) => rgba(*c),
            // Gradient/image text is Phase 4; skip the run rather than tint it wrong.
            _ => return,
        };
        let coeffs = transform.as_coeffs();
        let scale = (coeffs[0] * coeffs[0] + coeffs[1] * coeffs[1]).sqrt().max(1e-6) as f32;
        let glyphs = glyphs
            .iter()
            .map(|g| {
                let p = transform * kurbo::Point::new(f64::from(g.x), f64::from(g.y));
                Glyph { id: g.id, x: p.x as f32, y: p.y as f32 }
            })
            .collect();
        self.glyphs.push(GlyphRun {
            font: font.clone(),
            font_size: font_size * scale,
            font_weight,
            color,
            glyphs,
            scissor: self.scissor,
            synthesize_italic: glyph_transform.is_some(),
        });
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

    fn image(&mut self, image: &peniko::ImageBrush, transform: Affine, index: u32) {
        let w = image.image.width as f64;
        let h = image.image.height as f64;
        let c00 = transform * kurbo::Point::new(0.0, 0.0);
        let c10 = transform * kurbo::Point::new(w, 0.0);
        let c11 = transform * kurbo::Point::new(w, h);
        let c01 = transform * kurbo::Point::new(0.0, h);
        let vertices = [
            ImageVertex { position: [c00.x as f32, c00.y as f32], uv: [0.0, 0.0] },
            ImageVertex { position: [c10.x as f32, c10.y as f32], uv: [1.0, 0.0] },
            ImageVertex { position: [c11.x as f32, c11.y as f32], uv: [1.0, 1.0] },
            ImageVertex { position: [c01.x as f32, c01.y as f32], uv: [0.0, 1.0] },
        ];
        self.images.push(ImageQuad { image_index: index, vertices, scissor: self.scissor });
    }

    fn shadow(&mut self, rect: &Rect, transform: Affine, radius: f64, std_dev: f64, color: Color) {
        let coeffs = transform.as_coeffs();
        let scale = (coeffs[0] * coeffs[0] + coeffs[1] * coeffs[1]).sqrt().max(1e-6);
        let c00 = transform * kurbo::Point::new(rect.x0, rect.y0);
        let c11 = transform * kurbo::Point::new(rect.x1, rect.y1);
        let c10 = transform * kurbo::Point::new(rect.x1, rect.y0);
        let c01 = transform * kurbo::Point::new(rect.x0, rect.y1);
        let rect = [
            c00.x.min(c10.x).min(c11.x).min(c01.x) as f32,
            c00.y.min(c10.y).min(c11.y).min(c01.y) as f32,
            c00.x.max(c10.x).max(c11.x).max(c01.x) as f32,
            c00.y.max(c10.y).max(c11.y).max(c01.y) as f32,
        ];
        let radius = (radius * scale) as f32;
        let sigma = (std_dev * scale) as f32;
        let color = rgba(color);
        // Inflate the quad by ~3σ to cover the gaussian blur extent.
        let extend = 3.0 * sigma;
        let quad = [
            [rect[0] - extend, rect[1] - extend],
            [rect[2] + extend, rect[1] - extend],
            [rect[2] + extend, rect[3] + extend],
            [rect[0] - extend, rect[3] + extend],
        ];
        let vert = |p: [f32; 2]| ShadowVertex { position: p, rect, radius, sigma, color };
        self.shadows.extend_from_slice(&[
            vert(quad[0]),
            vert(quad[1]),
            vert(quad[2]),
            vert(quad[0]),
            vert(quad[2]),
            vert(quad[3]),
        ]);
    }
}

fn emit(ops: &[Op], outer: Affine, tess: &mut Tess) {
    for op in ops {
        match op {
            Op::Fill { style, transform, brush: Brush::Solid(color), path, .. } => {
                tess.fill(path, outer * *transform, *style, *color);
            }
            Op::Stroke { style, transform, brush: Brush::Solid(color), path, .. } => {
                tess.stroke(path, outer * *transform, style, *color);
            }
            Op::PushLayer { blend, alpha, transform, clip, .. } => {
                let scissor = if *blend == peniko::BlendMode::default() && *alpha >= 1.0 {
                    scissor_rect(clip, outer * *transform)
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
            Op::Fragment { transform, ops } => {
                emit(ops, outer * transform.unwrap_or(Affine::IDENTITY), tess);
            }
            Op::Image { image, transform } if image.image.width != 0 && image.image.height != 0 => {
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

/// If `path` (a recorded rect clip) maps to another axis-aligned rectangle under
/// `transform` (translate + scale, no rotation/shear), return its physical-pixel
/// scissor rect; otherwise `None` (non-rect clips are Phase 4's job).
fn scissor_rect(path: &BezPath, transform: Affine) -> Option<Scissor> {
    let elements = path.elements();
    if elements.len() != 5 {
        return None;
    }
    let corner = |el: &kurbo::PathEl| -> Option<kurbo::Point> {
        match *el {
            kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) => Some(p),
            _ => None,
        }
    };
    let p0 = corner(&elements[0])?;
    let p1 = corner(&elements[1])?;
    let p2 = corner(&elements[2])?;
    let p3 = corner(&elements[3])?;
    if !matches!(elements[4], kurbo::PathEl::ClosePath) {
        return None;
    }
    let a = transform * p0;
    let b = transform * p1;
    let c = transform * p2;
    let d = transform * p3;
    // Axis-aligned iff opposite edges stay parallel to the axes.
    const EPS: f64 = 1e-3;
    if (a.y - b.y).abs() > EPS
        || (c.y - d.y).abs() > EPS
        || (b.x - c.x).abs() > EPS
        || (d.x - a.x).abs() > EPS
    {
        return None;
    }
    let min_x = a.x.min(b.x).min(c.x).min(d.x);
    let max_x = a.x.max(b.x).max(c.x).max(d.x);
    let min_y = a.y.min(b.y).min(c.y).min(d.y);
    let max_y = a.y.max(b.y).max(c.y).max(d.y);
    Some(Scissor {
        x: min_x.floor().max(0.0) as u32,
        y: min_y.floor().max(0.0) as u32,
        width: (max_x.ceil() - min_x.floor()).max(0.0) as u32,
        height: (max_y.ceil() - min_y.floor()).max(0.0) as u32,
    })
}

fn transformed(path: &BezPath, transform: Affine) -> BezPath {
    let mut screen = path.clone();
    screen.apply_affine(transform);
    screen
}

fn to_lyon_path(path: &BezPath) -> Path {
    let mut builder = Path::builder();
    let mut open = false;
    for element in path.elements() {
        match *element {
            kurbo::PathEl::MoveTo(to) => {
                if open {
                    builder.end(false);
                }
                builder.begin(point(to.x as f32, to.y as f32));
                open = true;
            }
            kurbo::PathEl::LineTo(to) if open => {
                builder.line_to(point(to.x as f32, to.y as f32));
            }
            kurbo::PathEl::QuadTo(ctrl, to) if open => {
                builder.quadratic_bezier_to(
                    point(ctrl.x as f32, ctrl.y as f32),
                    point(to.x as f32, to.y as f32),
                );
            }
            kurbo::PathEl::CurveTo(a, b, to) if open => {
                builder.cubic_bezier_to(
                    point(a.x as f32, a.y as f32),
                    point(b.x as f32, b.y as f32),
                    point(to.x as f32, to.y as f32),
                );
            }
            kurbo::PathEl::ClosePath if open => {
                builder.end(true);
                open = false;
            }
            _ => {}
        }
    }
    if open {
        builder.end(false);
    }
    builder.build()
}

fn rgba(color: Color) -> [f32; 4] {
    let [r, g, b, a] = color.components;
    [r * a, g * a, b * a, a]
}

struct SolidVertex([f32; 4]);
impl FillVertexConstructor<Vertex> for SolidVertex {
    fn new_vertex(&mut self, vertex: FillVertex<'_>) -> Vertex {
        let p = vertex.position();
        Vertex { position: [p.x, p.y], color: self.0 }
    }
}

pub fn scene() -> Scene {
    Scene::new()
}

/// The unchanged RenderObject-facing recording API. Only consumption of `Op` is
/// backend-specific, keeping later tessellation work out of widgets and layout.
pub struct Painter<'a> {
    scene: &'a mut Scene,
}

impl<'a> Painter<'a> {
    pub fn new(scene: &'a mut Scene) -> Self {
        Self { scene }
    }

    pub fn reborrow(&mut self) -> Painter<'_> {
        Painter { scene: self.scene }
    }

    pub fn scene_mut(&mut self) -> &mut Scene {
        self.scene
    }

    pub fn reset(&mut self) {
        self.scene.reset();
    }

    pub fn fill<'b>(
        &mut self,
        style: Fill,
        transform: Affine,
        brush: impl Into<BrushRef<'b>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        self.scene.ops.push(Op::Fill {
            style,
            transform,
            brush: brush.into().to_owned(),
            brush_transform,
            path: shape.to_path(TOL),
        });
    }

    pub fn stroke<'b>(
        &mut self,
        style: &Stroke,
        transform: Affine,
        brush: impl Into<BrushRef<'b>>,
        brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        self.scene.ops.push(Op::Stroke {
            style: style.clone(),
            transform,
            brush: brush.into().to_owned(),
            brush_transform,
            path: shape.to_path(TOL),
        });
    }

    pub fn push_layer<'s>(
        &mut self,
        clip_style: impl Into<peniko::StyleRef<'s>>,
        blend: impl Into<peniko::BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        self.scene.ops.push(Op::PushLayer {
            clip_fill: clip_fill_rule(clip_style.into()),
            blend: blend.into(),
            alpha,
            transform,
            clip: clip.to_path(TOL),
        });
    }

    pub fn pop_layer(&mut self) {
        self.scene.ops.push(Op::PopLayer);
    }

    pub fn draw_image<'b>(&mut self, image: impl Into<peniko::ImageBrushRef<'b>>, transform: Affine) {
        self.scene.ops.push(Op::Image { image: image.into().to_owned(), transform });
    }

    pub fn draw_blurred_rounded_rect(
        &mut self,
        transform: Affine,
        rect: Rect,
        brush: Color,
        radius: f64,
        std_dev: f64,
    ) {
        self.scene.ops.push(Op::BlurredRect { transform, rect, brush, radius, std_dev });
    }

    pub fn append(&mut self, fragment: &Scene, transform: Option<Affine>) {
        self.scene.ops.push(Op::Fragment { transform, ops: Rc::new(fragment.ops.clone()) });
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_glyphs(
        &mut self,
        font: &FontData,
        font_size: f32,
        font_weight: u16,
        normalized_coords: &[NormalizedCoord],
        brush: &Brush,
        transform: Affine,
        glyph_transform: Option<Affine>,
        style: Fill,
        glyphs: impl Iterator<Item = Glyph>,
    ) {
        self.scene.ops.push(Op::Glyphs {
            font: font.clone(),
            font_size,
            font_weight,
            normalized_coords: normalized_coords.to_vec(),
            brush: brush.clone(),
            transform,
            glyph_transform,
            style,
            glyphs: glyphs.collect(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appended_fragment_remains_a_single_retained_operation() {
        let mut fragment = Scene::new();
        Painter::new(&mut fragment).fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(0, 0, 0, 255),
            None,
            &Rect::new(0.0, 0.0, 10.0, 10.0),
        );

        let mut scene = Scene::new();
        scene.append(&fragment, Some(Affine::translate((4.0, 8.0))));

        assert_eq!(fragment.len(), 1);
        assert_eq!(scene.len(), 1);
    }

    #[test]
    fn solid_fill_tessellates_to_a_single_unclipped_batch() {
        let mut scene = Scene::new();
        Painter::new(&mut scene).fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(255, 0, 0, 255),
            None,
            &Rect::new(0.0, 0.0, 10.0, 10.0),
        );

        let mesh = scene.mesh().solid;
        assert!(!mesh.is_empty());
        assert_eq!(mesh.batches.len(), 1);
        assert_eq!(mesh.batches[0].scissor, None);
        assert!(mesh.batches[0].range.end > mesh.batches[0].range.start);
    }

    #[test]
    fn rect_clip_partitions_geometry_into_scissored_batches() {
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        // A scroll-viewport-style clip: an axis-aligned rect under a DPI scale.
        p.push_layer(
            Fill::NonZero,
            peniko::BlendMode::default(),
            1.0,
            Affine::IDENTITY,
            &Rect::new(0.0, 0.0, 100.0, 50.0),
        );
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(0, 255, 0, 255),
            None,
            &Rect::new(0.0, 0.0, 50.0, 25.0),
        );
        p.pop_layer();
        // After the clip, an unclipped fill must land in its own batch.
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(0, 0, 255, 255),
            None,
            &Rect::new(0.0, 0.0, 10.0, 10.0),
        );

        let mesh = scene.mesh().solid;
        assert_eq!(mesh.batches.len(), 2);
        let clipped = &mesh.batches[0];
        let unclipped = &mesh.batches[1];
        assert_eq!(unclipped.scissor, None);
        let scissor = clipped.scissor.expect("clipped batch has a scissor");
        assert_eq!((scissor.x, scissor.y, scissor.width, scissor.height), (0, 0, 100, 50));
    }

    #[test]
    fn rotated_clip_is_not_scissored() {
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        p.push_layer(
            Fill::NonZero,
            peniko::BlendMode::default(),
            1.0,
            Affine::rotate(std::f64::consts::FRAC_PI_4),
            &Rect::new(0.0, 0.0, 100.0, 50.0),
        );
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(0, 255, 0, 255),
            None,
            &Rect::new(0.0, 0.0, 50.0, 25.0),
        );
        p.pop_layer();

        let mesh = scene.mesh().solid;
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
            &Rect::new(0.0, 0.0, 100.0, 40.0),
        );

        let mesh = scene.mesh().solid;
        assert!(!mesh.is_empty());
        // Dashes split the outline into multiple disjoint pieces, so the vertex
        // count exceeds the 4 corners a plain rect stroke would produce.
        assert!(mesh.vertices.len() > 4);
    }
}
