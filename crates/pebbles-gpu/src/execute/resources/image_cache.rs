//! The image cache (10 §6): textures keyed by `Blob::id` — never a pointer, so no ABA — kept
//! alive by a weak reference to the app's pixels, uploaded once, premultiplied.
//!
//! `Blob::from_raw_parts` callers must keep id ↔ bytes 1:1, since the key trusts the id.

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

/// Dynamic images not drawn for this many frames are dropped (R37).
pub(crate) const DYNAMIC_IDLE_FRAMES: u64 = 120;

/// Scratch buffers above this are released after an upload (10 §6.2).
const IMAGE_SCRATCH_RETAIN_BYTES: usize = 16 << 20;

struct ImageEntry {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// One bind group per sampler key used with this image (≤ 27, 10 §3.4).
    bind_groups: Vec<(SamplerKey, wgpu::BindGroup)>,
    /// Static images: the app's pixels (`None` for dynamic images, which age out instead).
    weak: Option<peniko::WeakBlob<u8>>,
    /// Dynamic images (R37): the uploaded generation.
    generation: Option<u64>,
    last_used: u64,
    /// Uploaded texture bytes: every level of `w · h · 4` after any downscale.
    bytes: u64,
    /// Whether the full mip chain is resident (R35: `Medium`/`High` images).
    mips: bool,
}

/// Uploaded images by key, under a byte budget (10 §6, M02).
///
/// A GPU texture is freed at the first `end_frame` after the app drops its last `peniko::Blob`
/// clone, or earlier under budget pressure if it was not drawn this frame.
pub(crate) struct ImageCache {
    entries: HashMap<ImageKey, ImageEntry>,
    /// Σ `bytes` of every entry (running total, O(1)).
    total_bytes: u64,
    /// `GpuBudgets::image_cache_bytes`; soft for images drawn this frame (pinning, §6.3).
    budget_bytes: u64,
    scratch: Vec<u8>,
    /// Reused LRU sort buffer.
    evict_scratch: Vec<(u64, ImageKey, u64)>,
    evict_keys: Vec<ImageKey>,
    frame: u64,
    /// Running bind-group total for the census (10 §9.2: O(1) reads, never iteration).
    #[cfg(debug_assertions)]
    total_bind_groups: usize,
}

/// §6.3 budget step, pure: among `candidates` (`(last_used, key, bytes)`), those not used in
/// `frame` in LRU order, until `total − Σ bytes ≤ budget`. Pinned entries are never chosen.
pub(crate) fn select_evictions(
    candidates: &mut Vec<(u64, ImageKey, u64)>,
    total: u64,
    budget: u64,
    frame: u64,
    out: &mut Vec<ImageKey>,
) {
    out.clear();
    if total <= budget {
        return;
    }
    candidates.retain(|c| c.0 < frame);
    candidates.sort_unstable_by_key(|c| (c.0, c.1.blob_id));
    let mut total = total;
    for &(_, key, bytes) in candidates.iter() {
        if total <= budget {
            break;
        }
        out.push(key);
        total = total.saturating_sub(bytes);
    }
}

impl ImageCache {
    pub(crate) fn new(budget_bytes: u64) -> Self {
        Self {
            entries: HashMap::new(),
            total_bytes: 0,
            budget_bytes,
            scratch: Vec::new(),
            evict_scratch: Vec::new(),
            evict_keys: Vec::new(),
            frame: 0,
            #[cfg(debug_assertions)]
            total_bind_groups: 0,
        }
    }

    pub(crate) fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// The resident texture view of `key` (image brushes bind it with their own layout, 09 §6).
    pub(crate) fn view(&self, key: &ImageKey) -> Option<&wgpu::TextureView> {
        self.entries.get(key).map(|e| &e.view)
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
        #[cfg(debug_assertions)]
        {
            self.total_bind_groups += 1;
        }
    }

    /// Census fields owned by the image cache (M05).
    /// Σ bytes of the cached textures (10 §9.3).
    pub(crate) fn bytes(&self) -> u64 {
        self.total_bytes
    }

    #[cfg(debug_assertions)]
    pub(crate) fn census_into(&self, c: &mut crate::census::GpuCensus) {
        c.image_entries = self.entries.len();
        c.image_bytes = self.total_bytes;
        c.image_bind_groups = self.total_bind_groups;
    }

    /// Make `image` resident (uploading on a miss). Returns false when it cannot be drawn.
    pub(crate) fn ensure(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        image: &peniko::ImageData,
        mips: bool,
        stats: &mut FrameStats,
    ) -> bool {
        let key = ImageKey::of(image);
        if let Some(e) = self.entries.get_mut(&key) {
            e.last_used = self.frame;
            if e.mips || !mips {
                return true;
            }
            // First drawn `Low`, now filtered: re-insert with the mip chain (one re-upload, R35).
            if let Some(old) = self.entries.remove(&key) {
                self.total_bytes -= old.bytes;
                #[cfg(debug_assertions)]
                {
                    self.total_bind_groups -= old.bind_groups.len();
                }
            }
        }
        let bytes: &[u8] = image.data.data();
        let (w, h) = (image.width, image.height);
        // Prepare validated the data (R33); this stays as a guard, never counted twice.
        if w == 0 || h == 0 || u64::from(w) * u64::from(h) * 4 != bytes.len() as u64 {
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
        // S10: above the texture limit or ¼ of the image budget → downscale by k (miss only).
        let k = downscale_factor(w, h, max, self.budget_bytes / 4);
        let (pixels, w, h) = if k > 1 {
            stats.record(Unsupported::ImageTooLarge);
            let mut out = Vec::new();
            let (dw, dh) = downscale(pixels, w, h, k, &mut out);
            self.scratch = out;
            (self.scratch.as_slice(), dw, dh)
        } else {
            (pixels, w, h)
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pebbles.gpu.image"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: if mips { super::image_mips::mip_level_count(w, h) } else { 1 },
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
        let mut bytes = u64::from(w) * u64::from(h) * 4;
        if mips {
            // Every level uploaded once, at insert (R35); the budget counts them all.
            for (level, (px, lw, lh)) in super::image_mips::build_mips(pixels, w, h).iter().enumerate() {
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: u32::try_from(level + 1).unwrap_or(u32::MAX),
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    px,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * lw),
                        rows_per_image: None,
                    },
                    wgpu::Extent3d { width: *lw, height: *lh, depth_or_array_layers: 1 },
                );
                bytes += u64::from(*lw) * u64::from(*lh) * 4;
            }
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        if self.scratch.capacity() > IMAGE_SCRATCH_RETAIN_BYTES {
            self.scratch = Vec::new();
        }
        stats.images_uploaded += 1;
        stats.textures_created += 1;
        stats.texture_views_created += 1;
        self.total_bytes += bytes;
        stats.texture_bytes_uploaded += bytes;
        self.entries.insert(
            key,
            ImageEntry {
                _texture: texture,
                view,
                bind_groups: Vec::new(),
                weak: Some(image.data.downgrade()),
                generation: None,
                last_used: self.frame,
                bytes,
                mips,
            },
        );
        true
    }

    /// Make dynamic image `frame` resident (R37): first sight creates one texture; a new
    /// generation uploads only its dirty rectangle. Bytes count against the image budget.
    pub(crate) fn ensure_dynamic(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: ImageKey,
        frame: &pebbles_render::paint::DynamicImageFrame,
        stats: &mut FrameStats,
    ) -> bool {
        let (w, h) = (frame.width, frame.height);
        let max = device.limits().max_texture_dimension_2d;
        if w == 0
            || h == 0
            || w > max
            || h > max
            || frame.data.len() as u64 != u64::from(w) * u64::from(h) * 4
        {
            return false;
        }
        if let Some(e) = self.entries.get_mut(&key) {
            e.last_used = self.frame;
            if e.generation != Some(frame.generation) {
                let rect = super::dynamic_images::dirty_rect(frame);
                super::dynamic_images::upload_rect(queue, &e._texture, frame, rect, stats);
                e.generation = Some(frame.generation);
            }
            return true;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pebbles.gpu.image.dynamic"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let full = super::dynamic_images::full_rect(frame);
        super::dynamic_images::upload_rect(queue, &texture, frame, full, stats);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        stats.textures_created += 1;
        stats.texture_views_created += 1;
        let bytes = u64::from(w) * u64::from(h) * 4;
        self.total_bytes += bytes;
        self.entries.insert(
            key,
            ImageEntry {
                _texture: texture,
                view,
                bind_groups: Vec::new(),
                weak: None,
                generation: Some(frame.generation),
                last_used: self.frame,
                bytes,
                mips: false,
            },
        );
        true
    }

    /// After the last submit (10 §6.3): liveness first, then the budget with pinning.
    pub(crate) fn end_frame(&mut self, stats: &mut FrameStats) {
        let before = self.entries.len();
        #[cfg(debug_assertions)]
        let mut groups = self.total_bind_groups;
        let mut bytes = self.total_bytes;
        self.entries.retain(|_, e| {
            let live = match &e.weak {
                Some(w) => w.upgrade().is_some(),
                // Dynamic images (R37) are dropped when not drawn for DYNAMIC_IDLE_FRAMES.
                None => self.frame.saturating_sub(e.last_used) < DYNAMIC_IDLE_FRAMES,
            };
            if !live {
                bytes -= e.bytes;
                #[cfg(debug_assertions)]
                {
                    groups -= e.bind_groups.len();
                }
            }
            live
        });
        if bytes > self.budget_bytes {
            self.evict_scratch.clear();
            self.evict_scratch.extend(self.entries.iter().map(|(k, e)| (e.last_used, *k, e.bytes)));
            select_evictions(
                &mut self.evict_scratch,
                bytes,
                self.budget_bytes,
                self.frame,
                &mut self.evict_keys,
            );
            for key in &self.evict_keys {
                if let Some(e) = self.entries.remove(key) {
                    bytes -= e.bytes;
                    #[cfg(debug_assertions)]
                    {
                        groups -= e.bind_groups.len();
                    }
                }
            }
        }
        self.total_bytes = bytes;
        #[cfg(debug_assertions)]
        {
            self.total_bind_groups = groups;
        }
        stats.image_evictions += u32::try_from(before - self.entries.len()).unwrap_or(u32::MAX);
        stats.image_cache_bytes = self.total_bytes;
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
/// Smallest integer k ≥ 1 with ceil(w/k) ≤ max_side, ceil(h/k) ≤ max_side and
/// ceil(w/k)·ceil(h/k)·4 ≤ max_bytes (u64 math). C13's box filter then downscales by k (S10).
pub(crate) fn downscale_factor(w: u32, h: u32, max_side: u32, max_bytes: u64) -> u32 {
    let max_side = max_side.max(1);
    let mut k = w.max(h).div_ceil(max_side).max(1);
    loop {
        let (dw, dh) = (u64::from(w.div_ceil(k)), u64::from(h.div_ceil(k)));
        if dw * dh * 4 <= max_bytes || (dw == 1 && dh == 1) {
            return k;
        }
        k += 1;
    }
}

/// Integer-factor box filter in premultiplied space (10 §6.2): `w × h` → `ceil(w/k) × ceil(h/k)`.
fn downscale(src: &[u8], w: u32, h: u32, k: u32, out: &mut Vec<u8>) -> (u32, u32) {
    let k = k.max(1);
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
        assert_eq!(downscale(&src, 5, 2, downscale_factor(5, 2, 4, u64::MAX), &mut out), (3, 1));
        assert!(out.iter().all(|&b| b == 255));
    }

    #[test]
    fn box_downscale_averages_2x2_blocks() {
        // 2×2 of channel values 0, 100, 200, 101 → one texel (401 + 2) / 4 = 100.
        let src: Vec<u8> = [0u8, 100, 200, 101].iter().flat_map(|&v| [v, v, v, 255]).collect();
        let mut out = Vec::new();
        assert_eq!(downscale(&src, 2, 2, 2, &mut out), (1, 1));
        assert_eq!(out, vec![100, 100, 100, 255]);
    }

    #[test]
    fn downscale_factor_fits_side() {
        assert_eq!(downscale_factor(10000, 300, 8192, u64::MAX), 2);
    }

    #[test]
    fn downscale_factor_fits_bytes() {
        assert_eq!(downscale_factor(16384, 16384, 8192, 64 << 20), 4);
        assert_eq!(downscale_factor(64, 64, 8192, 64 << 20), 1, "small images are never touched");
    }
}

#[cfg(test)]
mod budget_tests {
    use super::{ImageKey, select_evictions};

    fn key(id: u64) -> ImageKey {
        ImageKey { blob_id: id, width: 1, height: 1, format: 0, alpha: 0 }
    }

    #[test]
    fn select_evictions_lru_order() {
        let mut c = vec![(5, key(1), 10), (2, key(2), 10), (7, key(3), 10), (3, key(4), 10)];
        let mut out = Vec::new();
        select_evictions(&mut c, 40, 20, 9, &mut out);
        assert_eq!(out, vec![key(2), key(4)], "oldest first, until within budget");
    }

    #[test]
    fn select_evictions_skips_pinned() {
        let mut c = vec![(9, key(1), 30), (4, key(2), 10)];
        let mut out = Vec::new();
        select_evictions(&mut c, 40, 5, 9, &mut out);
        assert_eq!(out, vec![key(2)], "the entry used this frame is never evicted");
    }

    #[test]
    fn select_evictions_noop_under_budget() {
        let mut c = vec![(1, key(1), 10)];
        let mut out = vec![key(9)];
        select_evictions(&mut c, 10, 10, 5, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn image_key_differs_for_equal_bytes() {
        let data = |b: peniko::Blob<u8>| peniko::ImageData {
            data: b,
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::Alpha,
            width: 1,
            height: 1,
        };
        let a = ImageKey::of(&data(peniko::Blob::from(vec![1u8, 2, 3, 4])));
        let b = ImageKey::of(&data(peniko::Blob::from(vec![1u8, 2, 3, 4])));
        assert_ne!(a, b);
    }
}

/// One app-owned texture registered with `Renderer::register_texture` (02 §11.1, R61): never
/// copied, never evicted by the budget — the owner unregisters it.
pub(crate) struct ExternalTexture {
    view: wgpu::TextureView,
    size: (u32, u32),
    bind_groups: Vec<(SamplerKey, wgpu::BindGroup)>,
}

/// The registered external textures, by `TextureId`.
#[derive(Default)]
pub(crate) struct ExternalTextures {
    entries: HashMap<u64, ExternalTexture>,
    next_id: u64,
}

impl ExternalTextures {
    pub(crate) fn register(&mut self, view: wgpu::TextureView, size: (u32, u32)) -> u64 {
        // Ids are never reused, so a stale id can never alias a newer texture.
        self.next_id += 1;
        self.entries.insert(self.next_id, ExternalTexture { view, size, bind_groups: Vec::new() });
        self.next_id
    }

    pub(crate) fn unregister(&mut self, id: u64) -> bool {
        self.entries.remove(&id).is_some()
    }

    pub(crate) fn size(&self, id: u64) -> Option<(u32, u32)> {
        self.entries.get(&id).map(|e| e.size)
    }

    /// Create the `(id, sampler)` bind group on first use (before any pass opens); false when
    /// `id` is not registered.
    pub(crate) fn ensure_bind_group(
        &mut self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        id: u64,
        sampler: SamplerKey,
        samplers: &mut SamplerCache,
        stats: &mut FrameStats,
    ) -> bool {
        let Some(entry) = self.entries.get_mut(&id) else { return false };
        if entry.bind_groups.iter().any(|(k, _)| *k == sampler) {
            return true;
        }
        let s = samplers.get_or_create(device, sampler, stats);
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pebbles.gpu.bind.external"),
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
        true
    }

    pub(crate) fn bind_group(&self, id: u64, sampler: SamplerKey) -> Option<&wgpu::BindGroup> {
        self.entries.get(&id)?.bind_groups.iter().find(|(k, _)| *k == sampler).map(|(_, g)| g)
    }
}
