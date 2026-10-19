//! The **transitional** hybrid consumer (feature `vello-hybrid`): replays the shared
//! display list into a live `vello_hybrid::Scene`. Kept as inherent `Scene` methods
//! (`flush`, `image_uploads`) because sibling demo capture harnesses call them until K10;
//! deleted whole at Phase 7 (X01).

use super::display_list::{Op, TOL};
use super::{Affine, Brush, peniko};

// ----- the flush: recorded op-list -> a live `vello_hybrid::Scene` (Phase 4) ---

impl super::Scene {
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
        emit_list(self.ops(), Affine::IDENTITY, out, resources, &mut img_idx);
    }

    /// The CPU pixel data for every image op, in the SAME order [`Self::flush`] visits
    /// them — so `image_uploads()[i]` is the texture the flush references as
    /// `TextureId(i)`. The shell uploads these to GPU textures and binds them. Zero-size
    /// images are skipped in BOTH walks so the indices stay aligned.
    pub fn image_uploads(&self) -> Vec<ImageUpload> {
        let mut out = Vec::new();
        collect_images(self.ops(), &mut out);
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
        for px in rgba8.as_chunks_mut::<4>().0 {
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

/// Replay an op list under `outer`. Projected layers (09 §10.7) change the transform of the ops
/// between their push and pop: this backend draws them through the projection's affine
/// approximation (logged once; wgpu renders the true perspective).
fn emit_list(
    ops: &[Op],
    mut outer: Affine,
    out: &mut vello_hybrid::Scene,
    resources: &mut vello_hybrid::Resources,
    img_idx: &mut u32,
) {
    let mut pushed: Vec<Option<Affine>> = Vec::new();
    for op in ops {
        match op {
            Op::PushProjectedLayer { projection, bounds, clip, .. } => {
                if !projection.is_affine() {
                    static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
                    if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                        pebbles_foundation::log::logf(
                            pebbles_foundation::log::Level::Warn,
                            pebbles_foundation::log::Cat::Gpu,
                            format_args!(
                                "vello-hybrid: perspective layers drawn with their affine approximation (wgpu, 09 §10.7)"
                            ),
                        );
                    }
                }
                let placed = outer * projection.to_affine();
                out.set_transform(placed);
                let clip_path = clip
                    .as_ref()
                    .map_or_else(|| kurbo::Shape::to_path(bounds, TOL), |c| c.to_path(TOL).into_owned());
                out.push_layer(Some(&clip_path), None, None, None, None);
                pushed.push(Some(outer));
                outer = placed;
            }
            Op::PopLayer => {
                out.pop_layer();
                if let Some(Some(prev)) = pushed.pop() {
                    outer = prev;
                }
            }
            Op::PushLayer { .. } | Op::PushEffectLayer { .. } | Op::PushBackdropFilter { .. } => {
                pushed.push(None);
                emit(op, outer, out, resources, img_idx);
            }
            _ => emit(op, outer, out, resources, img_idx),
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
        Op::Fill { style, transform, brush, brush_transform, shape, .. } => {
            out.set_transform(outer * *transform);
            out.set_fill_rule(*style);
            apply_paint_transform(out, *brush_transform);
            if apply_paint(out, brush) {
                let path = shape.to_path(TOL);
                out.fill_path(&path);
            }
            out.reset_paint_transform();
        }
        Op::Stroke { style, transform, brush, brush_transform, shape, .. } => {
            out.set_transform(outer * *transform);
            out.set_stroke(style.clone());
            apply_paint_transform(out, *brush_transform);
            if apply_paint(out, brush) {
                let path = shape.to_path(TOL);
                out.stroke_path(&path);
            }
            out.reset_paint_transform();
        }
        Op::PushLayer { clip_fill, blend, alpha, transform, clip, .. } => {
            out.set_transform(outer * *transform);
            out.set_fill_rule(*clip_fill);
            let clip_path = clip.to_path(TOL);
            out.push_layer(Some(&clip_path), Some(*blend), Some(*alpha), None, None);
        }
        Op::PushEffectLayer { clip, transform, .. } => {
            // DR-24 adoption rule: effects are wgpu-only; keep the clip so pops stay balanced.
            static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                pebbles_foundation::log::logf(
                    pebbles_foundation::log::Level::Warn,
                    pebbles_foundation::log::Cat::Gpu,
                    format_args!(
                        "vello-hybrid: layer effects are not supported; drawn unfiltered (wgpu, 09 §10)"
                    ),
                );
            }
            out.set_transform(outer * *transform);
            let clip_path = clip.as_ref().map(|c| c.to_path(TOL).into_owned());
            out.push_layer(clip_path.as_ref(), None, None, None, None);
        }
        Op::PushBackdropFilter { clip, transform, .. } => {
            // DR-24 adoption rule: wgpu-only; the children still draw, clipped.
            static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                pebbles_foundation::log::logf(
                    pebbles_foundation::log::Level::Warn,
                    pebbles_foundation::log::Cat::Gpu,
                    format_args!(
                        "vello-hybrid: backdrop filters are not supported; skipped (wgpu, 09 §10.4)"
                    ),
                );
            }
            out.set_transform(outer * *transform);
            let clip_path = clip.to_path(TOL).into_owned();
            out.push_layer(Some(&clip_path), None, None, None, None);
        }
        Op::PopLayer => out.pop_layer(),
        Op::Image { image, transform, .. } => {
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
        Op::BlurredFill { .. } | Op::BlurredGlyphs { .. } => {
            // DR-24 adoption rule: no blurred masks in the transitional backend.
            static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                pebbles_foundation::log::logf(
                    pebbles_foundation::log::Level::Warn,
                    pebbles_foundation::log::Cat::Gpu,
                    format_args!(
                        "vello-hybrid: blurred masks are not supported; skipped (wgpu draws them, 02 R42)"
                    ),
                );
            }
        }
        Op::DynamicImage { .. } => {
            // DR-24 adoption rule: no in-place texture updates in the transitional backend.
            static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                pebbles_foundation::log::logf(
                    pebbles_foundation::log::Level::Warn,
                    pebbles_foundation::log::Cat::Gpu,
                    format_args!(
                        "vello-hybrid: Op::DynamicImage is not supported; skipped (wgpu draws it, 02 R37)"
                    ),
                );
            }
        }
        Op::ImageNine { .. } => {
            // DR-24 adoption rule: the transitional backend has no nine-slice; log once, skip.
            static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                pebbles_foundation::log::logf(
                    pebbles_foundation::log::Level::Warn,
                    pebbles_foundation::log::Cat::Gpu,
                    format_args!(
                        "vello-hybrid: Op::ImageNine is not supported; skipped (wgpu draws it, 02 R36)"
                    ),
                );
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
            ..
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
            emit_list(ops, placed, out, resources, img_idx);
        }
        // Handled by `emit_list` (it owns the transform stack).
        Op::PushProjectedLayer { .. } => {}
        Op::Vertices { .. }
        | Op::Atlas { .. }
        | Op::Custom { .. }
        | Op::GpuImage { .. }
        | Op::ShaderFill { .. } => {
            // DR-24 adoption rule: meshes and sprite batches are wgpu-only (custom GPU
            // primitives too, DR-22).
            static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                pebbles_foundation::log::logf(
                    pebbles_foundation::log::Level::Warn,
                    pebbles_foundation::log::Cat::Gpu,
                    format_args!(
                        "vello-hybrid: meshes, sprite batches, custom primitives, GPU textures and shader brushes are not supported; skipped (wgpu, 02 §11–§12)"
                    ),
                );
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
        for px in data.as_chunks_mut::<4>().0 {
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
        let mut fragment = super::super::Scene::new();
        {
            let mut p = super::super::Painter::new(&mut fragment);
            p.draw_image(&tagged_image(20, 2, 2), Affine::IDENTITY);
        }
        let mut scene = super::super::Scene::new();
        {
            let mut p = super::super::Painter::new(&mut scene);
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
        let mut scene = super::super::Scene::new();
        {
            let mut p = super::super::Painter::new(&mut scene);
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
        let mut scene = super::super::Scene::new();
        {
            let mut p = super::super::Painter::new(&mut scene);
            p.draw_image(&img, Affine::IDENTITY);
        }
        let uploads = scene.image_uploads();
        assert_eq!(uploads[0].rgba8, vec![30, 20, 10, 255]);
    }
}
