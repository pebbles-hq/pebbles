//! Dynamic-image uploads (02 R37): only the dirty rectangle of a new generation is written,
//! straight from the frame's blob (the full image's row stride), with no CPU copy.

use pebbles_render::paint::{DynamicImageFrame, IRect};

use crate::stats::FrameStats;

/// `ImageKey::blob_id` tag of dynamic images: their ids come from their own counter and must
/// never collide with `Blob::id`s.
pub(crate) const DYNAMIC_KEY_BIT: u64 = 1 << 63;

/// The whole image.
pub(crate) fn full_rect(f: &DynamicImageFrame) -> IRect {
    IRect { x: 0, y: 0, w: f.width, h: f.height }
}

/// The frame's dirty rectangle clipped to the image (`None` = the whole image).
pub(crate) fn dirty_rect(f: &DynamicImageFrame) -> IRect {
    let Some(d) = f.dirty else { return full_rect(f) };
    let x = d.x.min(f.width);
    let y = d.y.min(f.height);
    IRect { x, y, w: d.w.min(f.width - x), h: d.h.min(f.height - y) }
}

/// Write `rect` of `f` into `texture` (one `write_texture`, full-image row stride).
pub(crate) fn upload_rect(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    f: &DynamicImageFrame,
    rect: IRect,
    stats: &mut FrameStats,
) {
    if rect.w == 0 || rect.h == 0 {
        return;
    }
    let offset = (u64::from(rect.y) * u64::from(f.width) + u64::from(rect.x)) * 4;
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d { x: rect.x, y: rect.y, z: 0 },
            aspect: wgpu::TextureAspect::All,
        },
        f.data.data(),
        wgpu::TexelCopyBufferLayout { offset, bytes_per_row: Some(4 * f.width), rows_per_image: None },
        wgpu::Extent3d { width: rect.w, height: rect.h, depth_or_array_layers: 1 },
    );
    stats.texture_bytes_uploaded += u64::from(rect.w) * u64::from(rect.h) * 4;
}
