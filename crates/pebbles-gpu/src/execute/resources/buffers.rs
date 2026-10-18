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
        let padded = len.next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
        if padded > self.capacity {
            let max = device.limits().max_buffer_size;
            let new_capacity = next_capacity(self.capacity, padded, max);
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
            let rem = (len % wgpu::COPY_BUFFER_ALIGNMENT) as usize;
            if rem == 0 {
                queue.write_buffer(buffer, 0, bytes);
            } else {
                // Pad the tail to COPY_BUFFER_ALIGNMENT; the extra bytes are never read.
                let split = bytes.len() - rem;
                queue.write_buffer(buffer, 0, &bytes[..split]);
                let mut tail = [0u8; 4];
                const _: () = assert!(wgpu::COPY_BUFFER_ALIGNMENT == 4);
                tail[..rem].copy_from_slice(&bytes[split..]);
                queue.write_buffer(buffer, split as u64, &tail);
            }
        }
        stats.buffer_bytes_uploaded += len;
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
    pub shape_instances: GpuArena,
    pub shadow_instances: GpuArena,
    pub glyphs: GpuArena,
    pub images: GpuArena,
}

impl FrameArenas {
    pub(crate) fn new() -> Self {
        Self {
            solid_vertices: GpuArena::new(wgpu::BufferUsages::VERTEX, "pebbles.gpu.buffer.vertices"),
            solid_indices: GpuArena::new(wgpu::BufferUsages::INDEX, "pebbles.gpu.buffer.indices"),
            shape_instances: GpuArena::new(wgpu::BufferUsages::VERTEX, "pebbles.gpu.buffer.shapes"),
            shadow_instances: GpuArena::new(wgpu::BufferUsages::VERTEX, "pebbles.gpu.buffer.shadows"),
            glyphs: GpuArena::new(wgpu::BufferUsages::VERTEX, "pebbles.gpu.buffer.glyphs"),
            images: GpuArena::new(wgpu::BufferUsages::VERTEX, "pebbles.gpu.buffer.images"),
        }
    }
}
