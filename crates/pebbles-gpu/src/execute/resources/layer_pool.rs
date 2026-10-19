//! Pooled offscreen targets for Tier C runs (R17) and, from L13, layers (09 §3.8). One pool
//! per render target, owned by its `FrameTargets`. The bookkeeping (`LayerPoolBook`) is pure.

/// Every dimension is rounded up to a multiple of this.
pub(crate) const LAYER_BUCKET_PX: u32 = 64;
/// Free targets unused for this many frames are dropped.
pub(crate) const LAYER_IDLE_FRAMES: u64 = 120;

/// Round `v` up to the bucket, at least one bucket, at most `max`.
pub(crate) fn bucket(v: u32, max: u32) -> u32 {
    v.max(1).div_ceil(LAYER_BUCKET_PX).saturating_mul(LAYER_BUCKET_PX).min(max.max(LAYER_BUCKET_PX))
}

/// Bookkeeping of one pooled target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Slot {
    pub size: (u32, u32),
    pub format: wgpu::TextureFormat,
    pub samples: u32,
    pub last_used_frame: u64,
    pub in_use: bool,
}

impl Slot {
    /// `w·h·4·(samples + 1)` (09 §3.8).
    pub(crate) fn bytes(&self) -> u64 {
        u64::from(self.size.0) * u64::from(self.size.1) * 4 * (u64::from(self.samples) + 1)
    }
}

/// Pure pool bookkeeping: bucketing, best fit, idle eviction, budget.
#[derive(Debug, Default)]
pub(crate) struct LayerPoolBook {
    pub slots: Vec<Slot>,
    pub frame: u64,
}

/// What `acquire` decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Acquired {
    /// Reuse the slot at this index.
    Reuse(usize),
    /// Create a new slot (already pushed at this index) of the bucketed size.
    Create(usize),
}

impl LayerPoolBook {
    /// The smallest free slot with the same `(format, samples)` that fits `size`; else a new one.
    pub(crate) fn acquire(
        &mut self,
        size: (u32, u32),
        format: wgpu::TextureFormat,
        samples: u32,
        max: u32,
    ) -> Acquired {
        let best = self
            .slots
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                !s.in_use
                    && s.format == format
                    && s.samples == samples
                    && s.size.0 >= size.0
                    && s.size.1 >= size.1
            })
            .min_by_key(|(_, s)| u64::from(s.size.0) * u64::from(s.size.1))
            .map(|(i, _)| i);
        let frame = self.frame;
        if let Some(i) = best {
            self.slots[i].in_use = true;
            self.slots[i].last_used_frame = frame;
            return Acquired::Reuse(i);
        }
        let size = (bucket(size.0, max), bucket(size.1, max));
        self.slots.push(Slot { size, format, samples, last_used_frame: frame, in_use: true });
        Acquired::Create(self.slots.len() - 1)
    }

    /// Return slot `i` to the free list mid-frame (its last use is already encoded): a later
    /// acquire this frame may reuse it, since passes run in encode order (09 §3.8).
    pub(crate) fn release(&mut self, i: usize) {
        if let Some(s) = self.slots.get_mut(i) {
            s.in_use = false;
        }
    }

    /// End the frame: release everything, drop idle slots, then the oldest free slots while
    /// over `budget`. Returns the dropped indices in descending order (remove in that order).
    pub(crate) fn end_frame(&mut self, budget: u64) -> Vec<usize> {
        for s in &mut self.slots {
            s.in_use = false;
        }
        let frame = self.frame;
        let mut drop: Vec<usize> = (0..self.slots.len())
            .filter(|&i| frame.saturating_sub(self.slots[i].last_used_frame) >= LAYER_IDLE_FRAMES)
            .collect();
        let mut total: u64 =
            self.slots.iter().enumerate().filter(|(i, _)| !drop.contains(i)).map(|(_, s)| s.bytes()).sum();
        let mut by_age: Vec<usize> = (0..self.slots.len()).filter(|i| !drop.contains(i)).collect();
        by_age.sort_by_key(|&i| self.slots[i].last_used_frame);
        for i in by_age {
            if total <= budget {
                break;
            }
            total -= self.slots[i].bytes();
            drop.push(i);
        }
        drop.sort_unstable_by(|a, b| b.cmp(a));
        for &i in &drop {
            self.slots.remove(i);
        }
        self.frame += 1;
        drop
    }

    /// Bytes of every slot.
    pub(crate) fn bytes(&self) -> u64 {
        self.slots.iter().map(Slot::bytes).sum()
    }
}

/// The textures of one pooled target.
pub(crate) struct LayerTextures {
    /// Some iff samples > 1.
    pub msaa: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// The resolve target (samples > 1) or the render target (samples == 1).
    pub color: (wgpu::Texture, wgpu::TextureView),
    /// The composite bind group of this layer (10 §3.4 "Layer source"), keyed by the composite
    /// params buffer's generation.
    pub composite_bg: Option<super::bind_groups::GenBindGroup>,
}

/// The pool: `LayerPoolBook` paired index-for-index with textures.
#[derive(Default)]
pub(crate) struct LayerPool {
    pub book: LayerPoolBook,
    textures: Vec<LayerTextures>,
}

impl LayerPool {
    /// Acquire a target of at least `size`; returns its index.
    pub(crate) fn acquire(
        &mut self,
        device: &wgpu::Device,
        size: (u32, u32),
        format: wgpu::TextureFormat,
        samples: u32,
        stats: &mut crate::stats::FrameStats,
    ) -> usize {
        let max = device.limits().max_texture_dimension_2d;
        match self.book.acquire(size, format, samples, max) {
            Acquired::Reuse(i) => i,
            Acquired::Create(i) => {
                let s = self.book.slots[i];
                let tex = |label, sample_count, usage| {
                    let t = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some(label),
                        size: wgpu::Extent3d { width: s.size.0, height: s.size.1, depth_or_array_layers: 1 },
                        mip_level_count: 1,
                        sample_count,
                        dimension: wgpu::TextureDimension::D2,
                        format,
                        usage,
                        view_formats: &[],
                    });
                    let v = t.create_view(&wgpu::TextureViewDescriptor::default());
                    (t, v)
                };
                let msaa = (samples > 1)
                    .then(|| tex("pebbles.gpu.layer.msaa", samples, wgpu::TextureUsages::RENDER_ATTACHMENT));
                let color = tex(
                    "pebbles.gpu.layer.color",
                    1,
                    wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::COPY_SRC,
                );
                stats.textures_created += 1 + u32::from(msaa.is_some());
                stats.texture_views_created += 1 + u32::from(msaa.is_some());
                self.textures.push(LayerTextures { msaa, color, composite_bg: None });
                i
            }
        }
    }

    /// See [`LayerPoolBook::release`].
    pub(crate) fn release(&mut self, i: usize) {
        self.book.release(i);
    }

    /// Pooled target count (census).
    pub(crate) fn len(&self) -> usize {
        self.textures.len()
    }

    pub(crate) fn textures(&self, i: usize) -> &LayerTextures {
        &self.textures[i]
    }

    pub(crate) fn textures_mut(&mut self, i: usize) -> &mut LayerTextures {
        &mut self.textures[i]
    }

    /// Release everything and evict per 09 §3.8 under `budget` (`GpuBudgets::layer_pool_bytes`).
    pub(crate) fn end_frame(&mut self, budget: u64) {
        for i in self.book.end_frame(budget) {
            self.textures.remove(i);
        }
    }

    /// Bytes held by the pool.
    pub(crate) fn bytes(&self) -> u64 {
        self.book.bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const F: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

    #[test]
    fn bucket_rounds_up_to_64() {
        let mut b = LayerPoolBook::default();
        b.acquire((100, 30), F, 4, 8192);
        assert_eq!(b.slots[0].size, (128, 64));
        assert_eq!(bucket(0, 8192), 64);
        assert_eq!(bucket(9000, 8192), 8192);
    }

    #[test]
    fn best_fit_prefers_smallest_area() {
        let mut b = LayerPoolBook::default();
        b.acquire((500, 500), F, 4, 8192);
        b.acquire((100, 100), F, 4, 8192);
        b.acquire((200, 200), F, 4, 8192);
        b.end_frame(u64::MAX);
        assert_eq!(b.acquire((90, 90), F, 4, 8192), Acquired::Reuse(1));
        assert_eq!(b.acquire((150, 150), F, 4, 8192), Acquired::Reuse(2));
        // Wrong sample count never matches.
        assert!(matches!(b.acquire((10, 10), F, 1, 8192), Acquired::Create(_)));
    }

    #[test]
    fn idle_targets_evicted_after_120_frames() {
        let mut b = LayerPoolBook::default();
        b.acquire((64, 64), F, 4, 8192); // used in frame 0
        for _ in 0..LAYER_IDLE_FRAMES {
            b.end_frame(u64::MAX); // frames 0..=119
        }
        assert_eq!(b.slots.len(), 1, "idle for 119 frames: kept");
        assert_eq!(b.end_frame(u64::MAX), vec![0], "idle for {LAYER_IDLE_FRAMES} frames: dropped");
    }

    #[test]
    fn budget_evicts_oldest_free_first() {
        let mut b = LayerPoolBook::default();
        b.acquire((64, 64), F, 1, 8192); // frame 0
        b.end_frame(u64::MAX);
        b.acquire((128, 64), F, 1, 8192); // frame 1, new (slot 0 too small)
        b.end_frame(u64::MAX);
        let one = b.slots[1].bytes();
        assert_eq!(b.end_frame(one), vec![0], "the older slot goes first");
        assert_eq!(b.slots.len(), 1);
        assert_eq!(b.slots[0].size, (128, 64));
    }
}
