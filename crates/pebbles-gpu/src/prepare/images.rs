//! Image preparation: the CPU pixel data for each image op, and the image quads.

use std::ops::Range;

use kurbo::Affine;
use pebbles_render::paint::Scene;
use pebbles_render::paint::display_list::Op;

use super::num::{index_u32, to_f32};
use super::plan::{FramePlan, ImageQuad};
use crate::gpu_types::ImageVertex;

/// CPU pixel data for one recorded image, handed to the shell for GPU upload.
pub(crate) struct ImageUpload {
    /// Stable identity (source pixel-buffer pointer) for the shell's texture cache.
    pub id: u64,
    pub width: u32,
    pub height: u32,
    /// Unmultiplied RGBA8 (Bgra8 sources are byte-swapped). Row-major, tightly packed.
    pub rgba8: Vec<u8>,
}

/// The CPU pixel data for every image op in `scene`, in the SAME order the prepare walk
/// visits them. Zero-size images are skipped in BOTH walks so the indices align.
pub(crate) fn image_uploads(scene: &Scene) -> Vec<ImageUpload> {
    let mut out = Vec::new();
    collect_images(scene.ops(), &mut out);
    out
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

/// Append one image quad (physical-pixel space) and return its arena range.
pub(crate) fn image(
    plan: &mut FramePlan,
    image: &peniko::ImageBrush,
    transform: Affine,
    index: u32,
    opacity: f32,
) -> Range<u32> {
    let w = image.image.width as f64;
    let h = image.image.height as f64;
    let c00 = transform * kurbo::Point::new(0.0, 0.0);
    let c10 = transform * kurbo::Point::new(w, 0.0);
    let c11 = transform * kurbo::Point::new(w, h);
    let c01 = transform * kurbo::Point::new(0.0, h);
    let px = |p: kurbo::Point| [to_f32(p.x).unwrap_or(0.0), to_f32(p.y).unwrap_or(0.0)];
    let vertices = [
        ImageVertex { position: px(c00), uv: [0.0, 0.0], opacity },
        ImageVertex { position: px(c10), uv: [1.0, 0.0], opacity },
        ImageVertex { position: px(c11), uv: [1.0, 1.0], opacity },
        ImageVertex { position: px(c01), uv: [0.0, 1.0], opacity },
    ];
    let start = index_u32(plan.images.len());
    plan.images.push(ImageQuad { image_index: index, vertices });
    start..start + 1
}
