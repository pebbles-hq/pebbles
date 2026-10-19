//! Persistent per-frame GPU buffers (arenas) and the capacity math (10 §2.2).

use crate::stats::FrameStats;

/// A persistent GPU buffer that is rewritten (never recreated) each frame (10 §2.2). Capacity
/// only changes on growth (doubling, capped) or after a quiet shrink window (4× hysteresis).
pub(crate) struct GpuArena {
    buffer: Option<wgpu::Buffer>,
    capacity: u64,
    usage: wgpu::BufferUsages,
    label: &'static str,
    /// `min(device.limits().max_buffer_size, budgets.arena_max_bytes)`, a multiple of 4.
    max_capacity: u64,
    /// Max bytes uploaded in the current shrink window.
    high_water: u64,
    /// Rendered frames in the current shrink window.
    window_frames: u32,
    /// +1 on every (re)create; bind groups that reference the buffer compare it.
    generation: u64,
    /// An upload beyond `max_capacity` was already logged.
    over_warned: bool,
}

impl GpuArena {
    pub(crate) const INITIAL_BYTES: u64 = 64 * 1024;
    pub(crate) const SHRINK_WINDOW_FRAMES: u32 = 600;
    pub(crate) const SHRINK_FACTOR: u64 = 4;

    pub(crate) fn new(usage: wgpu::BufferUsages, label: &'static str, max_capacity: u64) -> Self {
        Self {
            buffer: None,
            capacity: 0,
            usage,
            label,
            max_capacity: (max_capacity / 4 * 4).max(Self::INITIAL_BYTES),
            high_water: 0,
            window_frames: 0,
            generation: 0,
            over_warned: false,
        }
    }

    /// Allocated buffer bytes (census).
    pub(crate) fn capacity(&self) -> u64 {
        self.capacity
    }

    /// Recreations so far (bind groups over the buffer rebuild when it changes, M12).
    #[cfg_attr(not(test), allow(dead_code))] // read by the M12 bind-group rebuild
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    /// The backing buffer, if any has been allocated yet.
    pub(crate) fn buffer(&self) -> Option<&wgpu::Buffer> {
        self.buffer.as_ref()
    }

    fn recreate(&mut self, device: &wgpu::Device, capacity: u64, stats: &mut FrameStats) {
        self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(self.label),
            size: capacity,
            usage: self.usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
        self.capacity = capacity;
        self.generation += 1;
        stats.buffers_created += 1;
    }

    /// Write `bytes` once this frame (O2), growing the buffer when it no longer fits.
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
            let mut capacity = next_capacity(self.capacity, padded, self.max_capacity);
            if capacity < padded {
                // Over the arena budget: correctness first — grow to exactly what this frame
                // needs (up to the device limit), and say so once.
                capacity = padded.min(device.limits().max_buffer_size);
                if !std::mem::replace(&mut self.over_warned, true) {
                    pebbles_foundation::log::logf(
                        pebbles_foundation::log::Level::Warn,
                        pebbles_foundation::log::Cat::Gpu,
                        format_args!(
                            "{}: {padded} B exceeds the arena budget of {} B (10 §2.2)",
                            self.label, self.max_capacity
                        ),
                    );
                }
            }
            if capacity < padded {
                return;
            }
            self.recreate(device, capacity, stats);
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
        self.high_water = self.high_water.max(padded);
    }

    /// After the frame's submit: close the shrink window every `SHRINK_WINDOW_FRAMES` frames,
    /// shrinking when the window's high water leaves 4× headroom.
    pub(crate) fn end_frame(&mut self, device: &wgpu::Device, stats: &mut FrameStats) {
        self.window_frames += 1;
        if self.window_frames < Self::SHRINK_WINDOW_FRAMES {
            return;
        }
        if self.buffer.is_some()
            && let Some(t) = shrink_target(self.capacity, self.high_water)
        {
            self.recreate(device, t, stats);
        }
        self.window_frames = 0;
        self.high_water = 0;
    }
}

/// Pure capacity math: grow by doubling, at least `INITIAL_BYTES`, capped at `max`.
pub(crate) fn next_capacity(current: u64, needed: u64, max: u64) -> u64 {
    needed.next_power_of_two().max(current.saturating_mul(2)).max(GpuArena::INITIAL_BYTES).min(max)
}

/// `Some(new_capacity)` when the arena should shrink at the end of a window (4× headroom).
pub(crate) fn shrink_target(capacity: u64, high_water: u64) -> Option<u64> {
    let target = high_water.next_power_of_two().max(GpuArena::INITIAL_BYTES);
    (capacity >= target.saturating_mul(GpuArena::SHRINK_FACTOR)).then_some(target)
}

/// One persistent arena per `FramePlan` array that exists at this card.
pub(crate) struct FrameArenas {
    pub solid_vertices: GpuArena,
    pub solid_indices: GpuArena,
    pub shape_instances: GpuArena,
    pub shadow_instances: GpuArena,
    pub glyphs: GpuArena,
    pub images: GpuArena,
    /// Mesh vertices (R63).
    pub meshes: GpuArena,
}

impl FrameArenas {
    fn all_mut(&mut self) -> [&mut GpuArena; 7] {
        [
            &mut self.solid_vertices,
            &mut self.solid_indices,
            &mut self.shape_instances,
            &mut self.shadow_instances,
            &mut self.glyphs,
            &mut self.images,
            &mut self.meshes,
        ]
    }

    /// Σ arena capacities (census).
    #[cfg(debug_assertions)]
    pub(crate) fn capacity_bytes(&self) -> u64 {
        [
            &self.solid_vertices,
            &self.solid_indices,
            &self.shape_instances,
            &self.shadow_instances,
            &self.glyphs,
            &self.images,
            &self.meshes,
        ]
        .iter()
        .map(|a| a.capacity())
        .sum()
    }

    /// Arenas capped at `min(device max_buffer_size, budgets.arena_max_bytes)` (10 §2.2).
    pub(crate) fn new(max_capacity: u64) -> Self {
        let a = |usage, label| GpuArena::new(usage, label, max_capacity);
        Self {
            solid_vertices: a(wgpu::BufferUsages::VERTEX, "pebbles.gpu.buffer.vertices"),
            solid_indices: a(wgpu::BufferUsages::INDEX, "pebbles.gpu.buffer.indices"),
            shape_instances: a(wgpu::BufferUsages::VERTEX, "pebbles.gpu.buffer.shapes"),
            shadow_instances: a(wgpu::BufferUsages::VERTEX, "pebbles.gpu.buffer.shadows"),
            glyphs: a(wgpu::BufferUsages::VERTEX, "pebbles.gpu.buffer.glyphs"),
            images: a(wgpu::BufferUsages::VERTEX, "pebbles.gpu.buffer.images"),
            meshes: a(wgpu::BufferUsages::VERTEX, "pebbles.gpu.buffer.meshes"),
        }
    }

    /// After the frame's last submit: every arena's shrink window (10 §2.2).
    pub(crate) fn end_frame(&mut self, device: &wgpu::Device, stats: &mut FrameStats) {
        for a in self.all_mut() {
            a.end_frame(device, stats);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: u64 = 1024 * 1024;

    #[test]
    fn next_capacity_doubles_and_caps() {
        assert_eq!(next_capacity(64 * 1024, 65 * 1024, 16 * MIB), 128 * 1024);
        assert_eq!(next_capacity(MIB, MIB + 4, 16 * MIB), 2 * MIB);
        assert_eq!(next_capacity(8 * MIB, 9 * MIB, 16 * MIB), 16 * MIB);
        assert_eq!(next_capacity(16 * MIB, 17 * MIB, 16 * MIB), 16 * MIB, "capped");
    }

    #[test]
    fn next_capacity_never_below_initial() {
        assert_eq!(next_capacity(0, 4, 16 * MIB), GpuArena::INITIAL_BYTES);
        assert_eq!(next_capacity(0, 1000, 16 * MIB), GpuArena::INITIAL_BYTES);
    }

    #[test]
    fn shrink_needs_4x_headroom() {
        assert_eq!(shrink_target(MIB, 256 * 1024), Some(256 * 1024));
        assert_eq!(shrink_target(MIB, 256 * 1024 + 4), None, "only 2× headroom: no shrink");
        assert_eq!(shrink_target(MIB, MIB), None);
    }

    #[test]
    fn shrink_never_below_initial() {
        assert_eq!(shrink_target(MIB, 1024), Some(GpuArena::INITIAL_BYTES));
        assert_eq!(shrink_target(GpuArena::INITIAL_BYTES, 0), None);
        assert_eq!(shrink_target(4 * GpuArena::INITIAL_BYTES, 0), Some(GpuArena::INITIAL_BYTES));
    }

    /// `segments_gpu::arena_grows_then_shrinks_after_window` (M06 AC2): one 1 MiB upload grows
    /// the arena; once a whole window has seen only 1 KiB uploads it returns to 64 KiB, with
    /// exactly two (re)creations and no validation error.
    #[test]
    #[ignore = "gpu"]
    fn arena_grows_then_shrinks_after_window() {
        let Some((ctx, id)) = crate::context::tests::gpu_or_skip(&crate::GpuInit::headless()) else { return };
        let gpu = ctx.device(id).expect("device");
        let (device, queue) = (gpu.device(), gpu.queue());
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let mut stats = FrameStats::default();
        let mut a = GpuArena::new(wgpu::BufferUsages::VERTEX, "pebbles.gpu.buffer.test", 16 * MIB);
        a.upload(device, queue, &vec![0u8; 1 << 20], &mut stats);
        a.end_frame(device, &mut stats);
        assert_eq!((a.capacity(), a.generation()), (MIB, 1));
        let small = vec![0u8; 1024];
        let mut frames = 0;
        while a.capacity() != GpuArena::INITIAL_BYTES && frames < 2 * GpuArena::SHRINK_WINDOW_FRAMES {
            a.upload(device, queue, &small, &mut stats);
            queue.submit([]);
            a.end_frame(device, &mut stats);
            frames += 1;
        }
        assert_eq!(a.capacity(), GpuArena::INITIAL_BYTES, "shrunk within two windows");
        // The window holding the 1 MiB frame cannot shrink; the next quiet window does.
        assert_eq!(frames, 2 * GpuArena::SHRINK_WINDOW_FRAMES - 1);
        assert_eq!(a.generation(), 2, "grown once, shrunk once");
        let err = pollster::block_on(scope.pop());
        assert!(err.is_none(), "{err:?}");
    }
}
