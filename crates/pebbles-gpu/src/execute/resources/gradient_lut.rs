//! The gradient LUT texture (09 §5.3, L34): baked ramps as rows of a `Rgba16Float` texture,
//! 256 texels per row (f16, not 8-bit: the §5.8 dither needs sub-LSB precision, L37). A ramp of `len` texels takes `len / 256` consecutive rows aligned to
//! that span (buddy-style). Pure bookkeeping (`LutBook`) is split from the GPU side
//! (`GradientLut`: texture, CPU mirror, uploads); growth re-uploads the mirror (no GPU copy).

use std::collections::HashMap;

use crate::prepare::gradient::RampKey;

/// Texels per LUT row.
pub(crate) const LUT_ROW_TEXELS: u32 = 256;
/// Initial rows.
pub(crate) const LUT_INITIAL_ROWS: u32 = 16;
/// Ramps unused this many frames are freed at frame end.
pub(crate) const GRADIENT_ROW_TTL_FRAMES: u64 = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RampSlot {
    pub first_row: u32,
    pub span: u32,
    pub len: u32,
    pub last_used_frame: u64,
}

/// Pure row allocation: occupancy, aligned first fit, TTL and LRU eviction, growth.
#[derive(Debug)]
pub(crate) struct LutBook {
    pub rows: u32,
    pub max_rows: u32,
    used: Vec<bool>,
    pub slots: HashMap<RampKey, RampSlot>,
}

/// What `allocate` decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Alloc {
    /// Placed at this first row.
    At(u32),
    /// Placed after growing the texture to `rows` (re-upload the mirror).
    Grown { first_row: u32, rows: u32 },
    /// No room even after growth and eviction.
    Full,
}

impl LutBook {
    pub(crate) fn new(max_rows: u32) -> Self {
        let rows = LUT_INITIAL_ROWS.min(max_rows.max(1));
        Self { rows, max_rows: max_rows.max(rows), used: vec![false; rows as usize], slots: HashMap::new() }
    }

    fn first_fit(&self, span: u32) -> Option<u32> {
        (0..self.rows)
            .step_by(span as usize)
            .find(|&r| r + span <= self.rows && (r..r + span).all(|i| !self.used[i as usize]))
    }

    fn mark(&mut self, first: u32, span: u32, on: bool) {
        for i in first..first + span {
            self.used[i as usize] = on;
        }
    }

    /// Place `key` (`len` texels) at `frame`: first fit; else grow (doubling) up to `max_rows`;
    /// else evict least-recently-used ramps not used this frame and retry.
    pub(crate) fn allocate(&mut self, key: RampKey, len: u32, frame: u64) -> Alloc {
        let span = (len / LUT_ROW_TEXELS).max(1);
        let mut grown = None;
        loop {
            if let Some(first) = self.first_fit(span) {
                self.mark(first, span, true);
                self.slots.insert(key, RampSlot { first_row: first, span, len, last_used_frame: frame });
                return match grown {
                    Some(rows) => Alloc::Grown { first_row: first, rows },
                    None => Alloc::At(first),
                };
            }
            if self.rows < self.max_rows {
                self.rows = (self.rows * 2).min(self.max_rows);
                self.used.resize(self.rows as usize, false);
                grown = Some(self.rows);
                continue;
            }
            let victim = self
                .slots
                .iter()
                .filter(|(_, s)| s.last_used_frame < frame)
                .min_by_key(|(_, s)| s.last_used_frame)
                .map(|(k, _)| k.clone());
            let Some(victim) = victim else { return Alloc::Full };
            self.free(&victim);
        }
    }

    fn free(&mut self, key: &RampKey) {
        if let Some(s) = self.slots.remove(key) {
            self.mark(s.first_row, s.span, false);
        }
    }

    /// Free ramps idle for `GRADIENT_ROW_TTL_FRAMES`.
    pub(crate) fn end_frame(&mut self, frame: u64) {
        let idle: Vec<RampKey> = self
            .slots
            .iter()
            .filter(|(_, s)| frame.saturating_sub(s.last_used_frame) > GRADIENT_ROW_TTL_FRAMES)
            .map(|(k, _)| k.clone())
            .collect();
        for k in &idle {
            self.free(k);
        }
    }

    #[cfg(any(test, debug_assertions))]
    pub(crate) fn rows_used(&self) -> u32 {
        u32::try_from(self.used.iter().filter(|&&u| u).count()).unwrap_or(u32::MAX)
    }
}

/// The GPU LUT: texture + CPU mirror + bookkeeping.
pub(crate) struct GradientLut {
    pub book: LutBook,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// Incremented whenever the texture is recreated (bind groups key on it).
    generation: u64,
    /// f16 bits.
    mirror: Vec<[u16; 4]>,
    scratch: Vec<[f32; 4]>,
    frame: u64,
}

fn create(device: &wgpu::Device, rows: u32) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.lut.gradient"),
        size: wgpu::Extent3d { width: LUT_ROW_TEXELS, height: rows, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        // §5.3: TEXTURE_BINDING | COPY_DST (+ COPY_SRC in tests, for the readback check).
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | if cfg!(test) { wgpu::TextureUsages::COPY_SRC } else { wgpu::TextureUsages::empty() },
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

impl GradientLut {
    pub(crate) fn new(device: &wgpu::Device, max_rows: u32) -> Self {
        let book = LutBook::new(max_rows);
        let (texture, view) = create(device, book.rows);
        let mirror = vec![[0u16; 4]; (book.rows * LUT_ROW_TEXELS) as usize];
        Self { book, texture, view, generation: 0, mirror, scratch: Vec::new(), frame: 0 }
    }

    pub(crate) fn view(&self) -> (&wgpu::TextureView, u64) {
        (&self.view, self.generation)
    }

    /// `(lut_start, lut_len)` of `key`: a hit touches it; a miss bakes it (via `bake`) and
    /// uploads its rows; `None` when the LUT is full (§5.3 "Full").
    pub(crate) fn resolve(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &RampKey,
        bake: impl FnOnce(&mut Vec<[f32; 4]>) -> u32,
        stats: &mut crate::stats::FrameStats,
    ) -> Option<(u32, u32)> {
        let frame = self.frame;
        if let Some(s) = self.book.slots.get_mut(key) {
            s.last_used_frame = frame;
            return Some((s.first_row * LUT_ROW_TEXELS, s.len));
        }
        let mut scratch = std::mem::take(&mut self.scratch);
        let len = bake(&mut scratch);
        stats.lut_bakes += 1;
        let alloc = self.book.allocate(key.clone(), len, frame);
        let first_row = match alloc {
            Alloc::Full => {
                self.scratch = scratch;
                return None;
            }
            Alloc::At(r) => r,
            Alloc::Grown { first_row, rows } => {
                (self.texture, self.view) = create(device, rows);
                self.generation += 1;
                self.mirror.resize((rows * LUT_ROW_TEXELS) as usize, [0u16; 4]);
                stats.textures_created += 1;
                stats.texture_views_created += 1;
                first_row
            }
        };
        let start = (first_row * LUT_ROW_TEXELS) as usize;
        let n = scratch.len().min(len as usize);
        for (dst, src) in self.mirror[start..start + n].iter_mut().zip(&scratch[..n]) {
            *dst = src.map(|v| half::f16::from_f32(v).to_bits());
        }
        let (from_row, rows) = match alloc {
            Alloc::Grown { rows, .. } => (0, rows),
            _ => (first_row, (len / LUT_ROW_TEXELS).max(1)),
        };
        let from = (from_row * LUT_ROW_TEXELS) as usize;
        let to = from + (rows * LUT_ROW_TEXELS) as usize;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: from_row, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(&self.mirror[from..to]),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(LUT_ROW_TEXELS * 8),
                rows_per_image: Some(rows),
            },
            wgpu::Extent3d { width: LUT_ROW_TEXELS, height: rows, depth_or_array_layers: 1 },
        );
        stats.gradient_rows_uploaded += rows;
        self.scratch = scratch;
        Some((first_row * LUT_ROW_TEXELS, len))
    }

    pub(crate) fn end_frame(&mut self) {
        self.book.end_frame(self.frame);
        self.frame += 1;
    }

    /// `(ramps, rows_used, rows_total)` (census).
    #[cfg(debug_assertions)]
    pub(crate) fn census(&self) -> (usize, u32, u32) {
        (self.book.slots.len(), self.book.rows_used(), self.book.rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(i: u32) -> RampKey {
        let g = peniko::Gradient::new_linear((0.0, 0.0), (1.0, 0.0)).with_stops(
            [peniko::Color::from_rgba8(i.to_le_bytes()[0], i.to_le_bytes()[1], 0, 255), peniko::Color::WHITE]
                .as_slice(),
        );
        RampKey::of(&g)
    }

    #[test]
    fn span_alignment() {
        let mut b = LutBook::new(64);
        assert_eq!(b.allocate(key(0), 256, 0), Alloc::At(0));
        let Alloc::At(first) = b.allocate(key(1), 1024, 0) else { panic!() };
        assert_eq!(first % 4, 0);
        assert_eq!(b.slots[&key(1)].span, 4);
    }

    #[test]
    fn hit_does_not_rebake() {
        let mut b = LutBook::new(64);
        let mut bakes = 0;
        for _ in 0..2 {
            if !b.slots.contains_key(&key(3)) {
                bakes += 1;
                b.allocate(key(3), 256, 0);
            }
        }
        assert_eq!(bakes, 1);
    }

    #[test]
    fn ttl_eviction_frees_rows() {
        let mut b = LutBook::new(64);
        b.allocate(key(0), 512, 0);
        assert_eq!(b.rows_used(), 2);
        b.end_frame(GRADIENT_ROW_TTL_FRAMES);
        assert_eq!(b.rows_used(), 2, "600 idle frames: kept");
        b.end_frame(GRADIENT_ROW_TTL_FRAMES + 1);
        assert_eq!(b.rows_used(), 0, "601 idle frames: freed");
    }

    #[test]
    fn grows_until_max_then_reports_full() {
        let mut b = LutBook::new(32);
        for i in 0..16 {
            assert!(matches!(b.allocate(key(i), 256, 0), Alloc::At(_)), "{i}");
        }
        assert_eq!(b.allocate(key(16), 256, 0), Alloc::Grown { first_row: 16, rows: 32 });
        for i in 17..32 {
            assert!(matches!(b.allocate(key(i), 256, 0), Alloc::At(_)));
        }
        // Every ramp was used this frame: nothing may be evicted.
        assert_eq!(b.allocate(key(99), 256, 0), Alloc::Full);
        // Next frame the least recently used ramp makes room.
        assert!(matches!(b.allocate(key(99), 256, 1), Alloc::At(_)));
    }

    /// AC5: after growth the texture equals the CPU mirror (rows re-uploaded, never GPU-copied).
    #[test]
    #[ignore = "gpu"]
    fn upload_roundtrip() {
        use crate::init::GpuInit;
        let Some((ctx, id)) = crate::context::tests::gpu_or_skip(&GpuInit::headless()) else { return };
        let gpu = ctx.device(id).expect("device");
        let (device, queue) = (gpu.device(), gpu.queue());
        let mut lut = GradientLut::new(device, 64);
        let mut stats = crate::stats::FrameStats::default();
        for i in 0..20 {
            let k = key(i);
            lut.resolve(device, queue, &k, |out| k.bake(out), &mut stats).expect("room");
        }
        assert!(lut.book.rows > LUT_INITIAL_ROWS, "grew");
        let rows = lut.book.rows;
        let bytes = u64::from(rows * LUT_ROW_TEXELS * 8);
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("pebbles.gpu.test.lut_readback"),
            size: bytes,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("pebbles.gpu.test.lut_readback"),
        });
        enc.copy_texture_to_buffer(
            lut.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(LUT_ROW_TEXELS * 8),
                    rows_per_image: Some(rows),
                },
            },
            wgpu::Extent3d { width: LUT_ROW_TEXELS, height: rows, depth_or_array_layers: 1 },
        );
        queue.submit([enc.finish()]);
        buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let data = buf.slice(..).get_mapped_range().to_vec();
        assert_eq!(data, bytemuck::cast_slice::<[u16; 4], u8>(&lut.mirror));
    }
}
