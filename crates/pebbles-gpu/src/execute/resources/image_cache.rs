//! The image cache (10 §6): textures keyed by `Blob::id` — never a pointer, so no ABA — kept
//! alive by a weak reference to the app's pixels, uploaded once, premultiplied.
//!
//! A GPU texture is freed at the first `end_frame` after the app drops its last `peniko::Blob`
//! clone. (The byte budget and LRU eviction land with M02.) `Blob::from_raw_parts` callers must
//! keep id ↔ bytes 1:1, since the key trusts the id.

use std::collections::HashMap;

use crate::stats::{FrameStats, Unsupported};

/// Image cache key (DR-14): blob identity plus everything that changes the uploaded texels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ImageKey {
    /// `ImageData.data.id()`.
    pub blob_id: u64,
    /// Source size, before any downscale.
    pub width: u32,
    pub height: u32,
    /// `peniko::ImageFormat` (Rgba8 = 0, Bgra8 = 1).
    pub format: u8,
    /// `peniko::ImageAlphaType` (Alpha = 0, AlphaPremultiplied = 1).
    pub alpha: u8,
}

impl ImageKey {
    pub(crate) fn of(image: &peniko::ImageData) -> Self {
        Self {
            blob_id: image.data.id(),
            width: image.width,
            height: image.height,
            format: u8::from(matches!(image.format, peniko::ImageFormat::Bgra8)),
            alpha: u8::from(matches!(image.alpha_type, peniko::ImageAlphaType::AlphaPremultiplied)),
        }
    }
}

use super::bind_groups::SamplerCache;
pub(crate) use super::bind_groups::SamplerKey;

/// Scratch buffers above this are released after an upload (10 §6.2).
const IMAGE_SCRATCH_RETAIN_BYTES: usize = 16 << 20;

struct ImageEntry {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// One bind group per sampler key used with this image (≤ 27, 10 §3.4).
    bind_groups: Vec<(SamplerKey, wgpu::BindGroup)>,
    weak: peniko::WeakBlob<u8>,
    last_used: u64,
}

/// Uploaded images by key.
pub(crate) struct ImageCache {
    entries: HashMap<ImageKey, ImageEntry>,
    scratch: Vec<u8>,
    frame: u64,
}

impl ImageCache {
    pub(crate) fn new() -> Self {
        Self { entries: HashMap::new(), scratch: Vec::new(), frame: 0 }
    }

    pub(crate) fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// The bind group for `key` sampled with `sampler`, if `ensure_bind_group` made it.
    pub(crate) fn bind_group(&self, key: &ImageKey, sampler: SamplerKey) -> Option<&wgpu::BindGroup> {
        self.entries.get(key)?.bind_groups.iter().find(|(k, _)| *k == sampler).map(|(_, g)| g)
    }

    /// Create the `(key, sampler)` bind group on first use (before any pass opens).
    pub(crate) fn ensure_bind_group(
        &mut self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        key: &ImageKey,
        sampler: SamplerKey,
        samplers: &mut SamplerCache,
        stats: &mut FrameStats,
    ) {
        let Some(entry) = self.entries.get_mut(key) else { return };
        if entry.bind_groups.iter().any(|(k, _)| *k == sampler) {
            return;
        }
        let s = samplers.get_or_create(device, sampler, stats);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pebbles.gpu.bind.image"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&entry.view),
                },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(s) },
            ],
        });
        stats.bind_groups_created += 1;
        entry.bind_groups.push((sampler, group));
    }

    /// Make `image` resident (uploading on a miss). Returns false when it cannot be drawn.
    pub(crate) fn ensure(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &peniko::ImageData,
        stats: &mut FrameStats,
    ) -> bool {
        let key = ImageKey::of(image);
        if let Some(e) = self.entries.get_mut(&key) {
            e.last_used = self.frame;
            return true;
        }
        let bytes: &[u8] = image.data.data();
        let (w, h) = (image.width, image.height);
        if w == 0 || h == 0 || u64::from(w) * u64::from(h) * 4 != bytes.len() as u64 {
            stats.record(Unsupported::ImageMalformed);
            return false;
        }
        let max = device.limits().max_texture_dimension_2d;
        let premultiplied = key.alpha == 1;
        let pixels: &[u8] = if key.format == 0 && premultiplied && w <= max && h <= max {
            bytes // zero copy
        } else {
            to_premultiplied_rgba(bytes, key.format == 1, premultiplied, &mut self.scratch);
            &self.scratch
        };
        let (pixels, w, h) = if w.max(h) > max {
            stats.record(Unsupported::ImageTooLarge);
            let mut out = Vec::new();
            let (dw, dh) = downscale(pixels, w, h, max, &mut out);
            self.scratch = out;
            (self.scratch.as_slice(), dw, dh)
        } else {
            (pixels, w, h)
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pebbles.gpu.image"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * w), rows_per_image: None },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        if self.scratch.capacity() > IMAGE_SCRATCH_RETAIN_BYTES {
            self.scratch = Vec::new();
        }
        stats.images_uploaded += 1;
        stats.textures_created += 1;
        stats.texture_views_created += 1;
        self.entries.insert(
            key,
            ImageEntry {
                _texture: texture,
                view,
                bind_groups: Vec::new(),
                weak: image.data.downgrade(),
                last_used: self.frame,
            },
        );
        true
    }

    /// After the last submit: drop every image the app no longer holds.
    pub(crate) fn end_frame(&mut self, stats: &mut FrameStats) {
        let before = self.entries.len();
        self.entries.retain(|_, e| e.weak.upgrade().is_some());
        stats.image_evictions += u32::try_from(before - self.entries.len()).unwrap_or(u32::MAX);
    }
}

/// Swizzle BGRA → RGBA and premultiply straight alpha into `out`.
fn to_premultiplied_rgba(src: &[u8], bgra: bool, premultiplied: bool, out: &mut Vec<u8>) {
    out.clear();
    out.extend_from_slice(src);
    for px in out.as_chunks_mut::<4>().0 {
        if bgra {
            px.swap(0, 2);
        }
        if !premultiplied {
            let a = u16::from(px[3]);
            for c in &mut px[..3] {
                *c = u8::try_from((u16::from(*c) * a + 127) / 255).unwrap_or(u8::MAX);
            }
        }
    }
}

/// Box-filter premultiplied RGBA down by an integer factor so both sides fit `max`.
fn downscale(src: &[u8], w: u32, h: u32, max: u32, out: &mut Vec<u8>) -> (u32, u32) {
    let k = w.max(h).div_ceil(max.max(1));
    let (dw, dh) = (w.div_ceil(k), h.div_ceil(k));
    out.clear();
    out.resize((dw * dh * 4) as usize, 0);
    for dy in 0..dh {
        for dx in 0..dw {
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for sy in dy * k..((dy + 1) * k).min(h) {
                for sx in dx * k..((dx + 1) * k).min(w) {
                    let i = ((sy * w + sx) * 4) as usize;
                    for c in 0..4 {
                        acc[c] += u32::from(src[i + c]);
                    }
                    n += 1;
                }
            }
            let o = ((dy * dw + dx) * 4) as usize;
            for c in 0..4 {
                out[o + c] = u8::try_from((acc[c] + n / 2) / n.max(1)).unwrap_or(u8::MAX);
            }
        }
    }
    (dw, dh)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(bytes: Vec<u8>) -> peniko::ImageData {
        peniko::ImageData {
            data: peniko::Blob::from(bytes),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::Alpha,
            width: 1,
            height: 1,
        }
    }

    #[test]
    fn image_key_uses_blob_id_not_pointer() {
        let a = image(vec![255, 0, 0, 255]);
        let b = image(vec![255, 0, 0, 255]);
        assert_ne!(ImageKey::of(&a), ImageKey::of(&b), "equal pixels, different blobs");
        let a2 = a.clone();
        assert_eq!(ImageKey::of(&a), ImageKey::of(&a2), "clone shares the blob");
    }

    #[test]
    fn premultiplies_straight_alpha_and_swizzles() {
        let mut out = Vec::new();
        to_premultiplied_rgba(&[0, 0, 255, 128], true, false, &mut out);
        assert_eq!(out, [128, 0, 0, 128]);
    }

    #[test]
    fn downscale_fits_the_limit() {
        let src = vec![255u8; 5 * 2 * 4];
        let mut out = Vec::new();
        assert_eq!(downscale(&src, 5, 2, 4, &mut out), (3, 1));
        assert!(out.iter().all(|&b| b == 255));
    }
}
