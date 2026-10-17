//! Persistent per-frame GPU buffers (arenas) and the capacity math (10 §2.2).

use crate::stats::FrameStats;

/// A persistent GPU buffer that is rewritten (never recreated) each frame. Capacity only
/// changes on growth (doubling). C12 lands grow-only; M06 adds the cap and shrink window.
pub(crate) struct GpuArena {
    buffer: Option<wgpu::Buffer>,
    capacity: u64,
    usage: wgpu::BufferUsages,
    label: &'static str,
}

impl GpuArena {
    pub(crate) const INITIAL_BYTES: u64 = 64 * 1024;

    pub(crate) fn new(usage: wgpu::BufferUsages, label: &'static str) -> Self {
        Self { buffer: None, capacity: 0, usage, label }
    }

    /// The backing buffer, if any has been allocated yet.
    pub(crate) fn buffer(&self) -> Option<&wgpu::Buffer> {
        self.buffer.as_ref()
    }

    /// Write `bytes` once this frame, growing the buffer (doubling) when it no longer fits.
    pub(crate) fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        bytes: &[u8],
        stats: &mut FrameStats,
    ) {
        let len = bytes.len() as u64;
        if len == 0 {
            return;
        }
        debug_assert_eq!(len % wgpu::COPY_BUFFER_ALIGNMENT as u64, 0);
        if len > self.capacity {
            let max = device.limits().max_buffer_size;
            let new_capacity = next_capacity(self.capacity, len, max);
            self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size: new_capacity,
                usage: self.usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.capacity = new_capacity;
            stats.buffers_created += 1;
        }
        if let Some(buffer) = &self.buffer {
            queue.write_buffer(buffer, 0, bytes);
        }
        stats.bytes_uploaded += len;
    }
}

/// Pure capacity math: grow by doubling, at least `INITIAL_BYTES`, capped at `max`.
pub(crate) fn next_capacity(current: u64, needed: u64, max: u64) -> u64 {
    needed.next_power_of_two().max(current.saturating_mul(2)).max(GpuArena::INITIAL_BYTES).min(max)
}

/// One persistent arena per `FramePlan` array that exists at this card.
pub(crate) struct FrameArenas {
    pub solid_vertices: GpuArena,
    pub solid_indices: GpuArena,
    pub shadow_instances: GpuArena,
    pub glyph_vertices: GpuArena,
    pub glyph_indices: GpuArena,
    pub image_vertices: GpuArena,
}

impl FrameArenas {
    pub(crate) fn new() -> Self {
        Self {
            solid_vertices: GpuArena::new(wgpu::BufferUsages::VERTEX, "pebbles.gpu.buffer.vertices"),
            solid_indices: GpuArena::new(wgpu::BufferUsages::INDEX, "pebbles.gpu.buffer.indices"),
            shadow_instances: GpuArena::new(wgpu::BufferUsages::VERTEX, "pebbles.gpu.buffer.shadows"),
            glyph_vertices: GpuArena::new(wgpu::BufferUsages::VERTEX, "pebbles.gpu.buffer.glyph.vertices"),
            glyph_indices: GpuArena::new(wgpu::BufferUsages::INDEX, "pebbles.gpu.buffer.glyph.indices"),
            image_vertices: GpuArena::new(wgpu::BufferUsages::VERTEX, "pebbles.gpu.buffer.image.vertices"),
        }
    }
}
