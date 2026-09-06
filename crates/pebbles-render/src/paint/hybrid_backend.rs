//! Backend B — Vello Hybrid (low-power). The [`Painter`] records each verb into a
//! retained, backend-neutral op-list; [`Scene::flush`] replays it into a
//! `vello_hybrid::Scene` on the GPU and [`Scene::image_uploads`] hands the shell the
//! pixels to bind as external textures. Selected by the `vello-hybrid` feature.

use std::rc::Rc;

use super::{Affine, BezPath, Brush, BrushRef, Color, Fill, FontData, Rect, Shape, Stroke, peniko};

/// Path-flattening tolerance for recorded shapes (px). The hybrid backend keeps
/// paths, not compute encodings, so we flatten once at record time.
const TOL: f64 = 0.1;

/// A shaped glyph — same `{id, x, y}` layout the widget layer builds for vello,
/// so the call sites are identical. Resolved to `vello_hybrid`'s glyph run at flush.
#[derive(Clone, Copy, Debug)]
pub struct Glyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}

/// A font variation coordinate (skrifa/vello both define this as `i16`).
pub type NormalizedCoord = i16;

/// The fill-rule for a recorded layer's clip path. Pebbles only ever clips with a fill
/// rule (never a stroked-outline clip), and `vello_hybrid`'s `push_layer` clip is a filled
/// path regardless — so a stroked clip-style degrades to `NonZero` at record time.
fn clip_fill_rule(s: peniko::StyleRef<'_>) -> Fill {
    match s {
        peniko::StyleRef::Fill(f) => f,
        peniko::StyleRef::Stroke(_) => Fill::NonZero,
    }
}

/// One recorded paint verb, owning its inputs. `Scene::flush` walks these into a live
/// `vello_hybrid::Scene` (resolving glyphs against `Resources` and images against bound
/// external textures), so every field is read there.
#[derive(Clone)]
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
        normalized_coords: Vec<NormalizedCoord>,
        brush: Brush,
        transform: Affine,
        glyph_transform: Option<Affine>,
        style: Fill,
        glyphs: Vec<Glyph>,
    },
    /// A retained fragment appended under `transform` (repaint boundaries,
    /// transformed subtrees). The ops are shared so re-appending each frame is cheap.
    Fragment {
        transform: Option<Affine>,
        ops: Rc<Vec<Op>>,
    },
}

/// A retained drawing buffer — a backend-neutral op-list. Filled by the [`Painter`];
/// the shell flushes it to a `vello_hybrid::Scene` on the GPU (Phase 4).
#[derive(Default, Clone)]
pub struct Scene {
    ops: Vec<Op>,
}

impl Scene {
    /// Create an empty [`Scene`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear all recorded ops (reuse the buffer for a fresh frame/fragment).
    pub fn reset(&mut self) {
        self.ops.clear();
    }

    /// Number of recorded ops (a fragment's re-append cost is one op).
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// Whether anything has been recorded.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Append another retained scene as a fragment under `transform` — the
    /// `Scene`-level twin of [`Painter::append`], used by the shell to compose the
    /// per-frame logical scene into the DPI-scaled frame. Mirrors `vello::Scene::append`.
    pub fn append(&mut self, fragment: &Scene, transform: Option<Affine>) {
        self.ops.push(Op::Fragment { transform, ops: Rc::new(fragment.ops.clone()) });
    }
}

/// Create an empty [`Scene`].
pub fn scene() -> Scene {
    Scene::new()
}

/// The drawing surface RenderObjects paint into — records each verb as an `Op`
/// on the retained list. Signatures mirror the vello backend exactly.
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

    /// The underlying scene (for the shell's flush + fragment plumbing).
    pub fn scene_mut(&mut self) -> &mut Scene {
        self.scene
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
        self.scene.ops.push(Op::Fill {
            style,
            transform,
            brush: brush.into().to_owned(),
            brush_transform,
            path: shape.to_path(TOL),
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
        self.scene.ops.push(Op::Stroke {
            style: style.clone(),
            transform,
            brush: brush.into().to_owned(),
            brush_transform,
            path: shape.to_path(TOL),
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
        self.scene.ops.push(Op::PushLayer {
            clip_fill: clip_fill_rule(clip_style.into()),
            blend: blend.into(),
            alpha,
            transform,
            clip: clip.to_path(TOL),
        });
    }

    /// Pop the most recent layer.
    pub fn pop_layer(&mut self) {
        self.scene.ops.push(Op::PopLayer);
    }

    /// Draw an image (already positioned by `transform`).
    pub fn draw_image<'b>(&mut self, image: impl Into<peniko::ImageBrushRef<'b>>, transform: Affine) {
        self.scene.ops.push(Op::Image { image: image.into().to_owned(), transform });
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
        self.scene.ops.push(Op::BlurredRect { transform, rect, brush, radius, std_dev });
    }

    /// Append a retained [`Scene`] fragment, translated by `transform`. The
    /// fragment's ops are shared (an `Rc`) so re-appending each frame is cheap.
    pub fn append(&mut self, fragment: &Scene, transform: Option<Affine>) {
        self.scene.ops.push(Op::Fragment { transform, ops: Rc::new(fragment.ops.clone()) });
    }

    /// Record a shaped glyph run (owned; the flush resolves glyphs against the
    /// hybrid atlas in `Resources`).
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
        self.scene.ops.push(Op::Glyphs {
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

// ----- the flush: recorded op-list -> a live `vello_hybrid::Scene` (Phase 4) ---

impl Scene {
    /// Replay the recorded op-list into a live `vello_hybrid::Scene` — the shell's
    /// GPU host then rasterizes THAT. This is the hybrid analogue of a `vello::Scene`
    /// already being GPU-ready: our `Scene` is a CPU command buffer, and `flush`
    /// resolves it (glyphs against the atlas in `resources`) into the backend scene.
    ///
    /// Images are the single deferred surface — `vello_hybrid` 0.2 exposes no stable
    /// public atlas-upload path — and are skipped with a one-time note (never wrong
    /// pixels; the image is simply absent until that lands).
    pub fn flush(&self, out: &mut vello_hybrid::Scene, resources: &mut vello_hybrid::Resources) {
        // Image ops draw an EXTERNAL texture keyed by its visit index; the shell binds
        // `TextureId(index)` from `image_uploads()` (same walk order) before rendering.
        let mut img_idx: u32 = 0;
        for op in &self.ops {
            emit(op, Affine::IDENTITY, out, resources, &mut img_idx);
        }
    }

    /// The CPU pixel data for every image op, in the SAME order [`Self::flush`] visits
    /// them — so `image_uploads()[i]` is the texture the flush references as
    /// `TextureId(i)`. The shell uploads these to GPU textures and binds them. Zero-size
    /// images are skipped in BOTH walks so the indices stay aligned.
    pub fn image_uploads(&self) -> Vec<ImageUpload> {
        let mut out = Vec::new();
        collect_images(&self.ops, &mut out);
        out
    }
}

/// CPU pixel data for one recorded image, handed to the shell for GPU upload.
pub struct ImageUpload {
    /// Stable-ish identity (source pixel-buffer pointer) for the shell's texture cache,
    /// so the same image isn't re-uploaded every frame.
    pub id: u64,
    pub width: u32,
    pub height: u32,
    /// Unmultiplied RGBA8 (Bgra8 sources are byte-swapped). Row-major, tightly packed.
    pub rgba8: Vec<u8>,
}

fn image_to_upload(img: &peniko::ImageBrush) -> ImageUpload {
    let d = &img.image;
    let bytes: &[u8] = d.data.as_ref();
    let mut rgba8 = bytes.to_vec();
    if matches!(d.format, peniko::ImageFormat::Bgra8) {
        for px in rgba8.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
    }
    ImageUpload { id: bytes.as_ptr() as usize as u64, width: d.width, height: d.height, rgba8 }
}

/// Collect image uploads in flush-visit order (ops in order, recursing fragments),
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

/// Set the active paint from a recorded brush. Returns `false` for image brushes
/// (deferred), so the caller can skip the draw rather than paint something wrong.
fn apply_paint(out: &mut vello_hybrid::Scene, brush: &Brush) -> bool {
    match brush {
        Brush::Solid(c) => {
            out.set_paint(*c);
            true
        }
        Brush::Gradient(g) => {
            out.set_paint(g.clone());
            true
        }
        Brush::Image(_) => {
            image_deferred_note();
            false
        }
    }
}

/// Replay one op under `outer` (the accumulated transform from enclosing fragments).
/// Every verb applies `outer * transform` via `set_transform`, which also carries the
/// clip/glyph/shadow geometry — so fragments (which have no `append` on this backend)
/// are handled by replaying their sub-ops with the fragment's placement folded in.
fn emit(
    op: &Op,
    outer: Affine,
    out: &mut vello_hybrid::Scene,
    resources: &mut vello_hybrid::Resources,
    img_idx: &mut u32,
) {
    match op {
        Op::Fill { style, transform, brush, brush_transform, path } => {
            out.set_transform(outer * *transform);
            out.set_fill_rule(*style);
            apply_paint_transform(out, *brush_transform);
            if apply_paint(out, brush) {
                out.fill_path(path);
            }
            out.reset_paint_transform();
        }
        Op::Stroke { style, transform, brush, brush_transform, path } => {
            out.set_transform(outer * *transform);
            out.set_stroke(style.clone());
            apply_paint_transform(out, *brush_transform);
            if apply_paint(out, brush) {
                out.stroke_path(path);
            }
            out.reset_paint_transform();
        }
        Op::PushLayer { clip_fill, blend, alpha, transform, clip } => {
            out.set_transform(outer * *transform);
            out.set_fill_rule(*clip_fill);
            out.push_layer(Some(clip), Some(*blend), Some(*alpha), None, None);
        }
        Op::PopLayer => out.pop_layer(),
        Op::Image { image, transform } => {
            let (w, h) = (image.image.width, image.image.height);
            if w != 0 && h != 0 {
                // The source rect (the whole image) maps to the destination by the op's
                // placement transform (set on the scene); the shell has bound this image
                // as `TextureId(*img_idx)` from `image_uploads()`.
                out.set_transform(outer * *transform);
                let source_region = vello_common::geometry::RectU16 {
                    x0: 0,
                    y0: 0,
                    x1: w.min(u16::MAX as u32) as u16,
                    y1: h.min(u16::MAX as u32) as u16,
                };
                out.draw_texture_rects(
                    vello_hybrid::TextureId(u64::from(*img_idx)),
                    peniko::ImageQuality::Medium,
                    [vello_hybrid::SampleRect { source_region, transform: Affine::IDENTITY }],
                );
                *img_idx += 1;
            }
        }
        Op::BlurredRect { transform, rect, brush, radius, std_dev } => {
            out.set_transform(outer * *transform);
            out.set_paint(*brush);
            out.fill_blurred_rounded_rect(rect, *radius as f32, *std_dev as f32, false);
        }
        Op::Glyphs {
            font,
            font_size,
            normalized_coords,
            brush,
            transform,
            glyph_transform,
            style,
            glyphs,
        } => {
            out.set_transform(outer * *transform);
            out.set_fill_rule(*style);
            if !apply_paint(out, brush) {
                return;
            }
            let run_glyphs = glyphs.iter().map(|g| glifo::Glyph { id: g.id, x: g.x, y: g.y });
            let mut run =
                out.glyph_run(resources, font).font_size(*font_size).normalized_coords(normalized_coords);
            if let Some(gt) = glyph_transform {
                run = run.glyph_transform(*gt);
            }
            run.fill_glyphs(run_glyphs);
        }
        Op::Fragment { transform, ops } => {
            let placed = outer * transform.unwrap_or(Affine::IDENTITY);
            for sub in ops.iter() {
                emit(sub, placed, out, resources, img_idx);
            }
        }
    }
}

fn apply_paint_transform(out: &mut vello_hybrid::Scene, bt: Option<Affine>) {
    match bt {
        Some(t) => out.set_paint_transform(t),
        None => out.reset_paint_transform(),
    }
}

/// One-time note for the rare case of an IMAGE used as a fill/stroke *brush* (a pattern
/// fill). `draw_image` (the common path) renders via `Op::Image`; only image *brushes* on
/// fills/strokes are skipped for now.
fn image_deferred_note() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        eprintln!(
            "pebbles-render(vello-hybrid): image *brush* fills are not supported yet \
             (draw_image renders normally). Skipping the pattern fill."
        );
    });
}
// The flush is verified end-to-end on real GPU by `spikes/renderer-bench` (it needs a
// `Resources`, whose only constructor is `Renderer::new`, so it can't be unit-tested
// CPU-only) — it records a Pebbles scene through the `Painter`, flushes it, and renders.

#[cfg(test)]
mod tests {
    use super::*;

    /// A `w×h` opaque image whose first pixel's red channel is `tag`, so uploads can be
    /// identified by their visit order.
    fn tagged_image(tag: u8, w: u32, h: u32) -> peniko::ImageBrush {
        let mut data = vec![0u8; (w * h * 4) as usize];
        for px in data.chunks_exact_mut(4) {
            px[3] = 255;
        }
        if !data.is_empty() {
            data[0] = tag;
        }
        peniko::ImageBrush {
            image: peniko::ImageData {
                data: peniko::Blob::from(data),
                format: peniko::ImageFormat::Rgba8,
                width: w,
                height: h,
                alpha_type: peniko::ImageAlphaType::Alpha,
            },
            sampler: peniko::ImageSampler::default(),
        }
    }

    /// `image_uploads()` MUST visit images in the same order `flush` assigns their
    /// `TextureId`s — including images nested inside appended fragments — or the shell
    /// would bind the wrong texture to each draw. This is the load-bearing invariant.
    #[test]
    fn image_uploads_follow_flush_visit_order_through_fragments() {
        let mut fragment = Scene::new();
        {
            let mut p = Painter::new(&mut fragment);
            p.draw_image(&tagged_image(20, 2, 2), Affine::IDENTITY);
        }
        let mut scene = Scene::new();
        {
            let mut p = Painter::new(&mut scene);
            p.draw_image(&tagged_image(10, 2, 2), Affine::IDENTITY); // TextureId(0)
            p.append(&fragment, None); // the fragment's image → TextureId(1)
            p.draw_image(&tagged_image(30, 2, 2), Affine::IDENTITY); // TextureId(2)
        }
        let uploads = scene.image_uploads();
        let tags: Vec<u8> = uploads.iter().map(|u| u.rgba8[0]).collect();
        assert_eq!(tags, vec![10, 20, 30], "uploads must be in depth-first visit order");
    }

    /// Zero-size images are skipped by BOTH `flush` and `image_uploads`, so the indices
    /// stay aligned (a skipped image must not consume a `TextureId`).
    #[test]
    fn zero_size_images_are_skipped_consistently() {
        let mut scene = Scene::new();
        {
            let mut p = Painter::new(&mut scene);
            p.draw_image(&tagged_image(5, 0, 0), Affine::IDENTITY); // skipped
            p.draw_image(&tagged_image(7, 2, 2), Affine::IDENTITY); // TextureId(0)
        }
        let uploads = scene.image_uploads();
        assert_eq!(uploads.len(), 1);
        assert_eq!(uploads[0].rgba8[0], 7);
    }

    /// `Bgra8` sources are byte-swapped to `Rgba8` on upload (external textures are
    /// `Rgba8Unorm`).
    #[test]
    fn bgra8_is_swapped_to_rgba8() {
        let mut img = tagged_image(0, 1, 1);
        // Set BGRA bytes = [b=10, g=20, r=30, a=255]; expect RGBA [30, 20, 10, 255].
        let bgra = vec![10u8, 20, 30, 255];
        img.image = peniko::ImageData {
            data: peniko::Blob::from(bgra),
            format: peniko::ImageFormat::Bgra8,
            width: 1,
            height: 1,
            alpha_type: peniko::ImageAlphaType::Alpha,
        };
        let mut scene = Scene::new();
        {
            let mut p = Painter::new(&mut scene);
            p.draw_image(&img, Affine::IDENTITY);
        }
        let uploads = scene.image_uploads();
        assert_eq!(uploads[0].rgba8, vec![30, 20, 10, 255]);
    }
}
