//! M33: the frame's buffer uploads, batched into ONE `Queue::write_buffer`.
//!
//! Every `write_buffer` makes wgpu create a staging buffer, and with the renderer's memory
//! budget threshold (11 §2.6 rule 5) every creation first queries the driver's memory budget —
//! on Mesa that reads `/proc` and sysfs, tens of µs per call. A frame used to issue one write per
//! uniform record set and per instance arena (≈ 8 on the dashboard). Instead, the frame's
//! writes are appended to one staging `Vec`, uploaded with a single `write_buffer` into a
//! persistent upload buffer, and copied to their destinations by a small command buffer that is
//! submitted right before the frame's own (same `submit` call). That is the order
//! `write_buffer` itself guarantees — before any command of the submit — so nothing changes for
//! the passes. On the GL backend (WebGL2 forbids copies between index and other buffers, and GL
//! has no budget query) every write goes straight to `write_buffer`, as before.
//!
//! [`FrameUploads::write_whole`] also remembers the bytes last written to each whole buffer
//! (by label and buffer generation) and skips a write that would store the same bytes again:
//! a buffer keeps its contents, so a static frame uploads nothing at all.

/// A pending copy: `size` bytes from `src` in the upload buffer to `dst` at `dst_offset`.
struct PendingCopy {
    dst: wgpu::Buffer,
    dst_offset: u64,
    /// Byte range in the staging `Vec` (and the upload buffer).
    src: std::ops::Range<usize>,
}

/// The frame's staged buffer writes (see the module docs).
pub(crate) struct FrameUploads {
    /// False on GL: write through immediately.
    batched: bool,
    staging: Vec<u8>,
    copies: Vec<PendingCopy>,
    /// Persistent `COPY_SRC | COPY_DST` buffer the staging is written into (grows only).
    upload: Option<wgpu::Buffer>,
    /// `write_whole`: per buffer label, the generation and bytes it holds.
    last: Vec<(&'static str, u64, Vec<u8>)>,
}

impl FrameUploads {
    pub(crate) fn new(batched: bool) -> Self {
        Self { batched, staging: Vec::new(), copies: Vec::new(), upload: None, last: Vec::new() }
    }

    /// [`write`](Self::write) `bytes` at offset 0 of the buffer `label` (generation
    /// `generation`: bumped whenever the buffer object is recreated), unless that buffer already
    /// holds exactly these bytes from an earlier write. Every write to the buffer must go
    /// through here, so the remembered bytes are its contents.
    pub(crate) fn write_whole(
        &mut self,
        label: &'static str,
        generation: u64,
        queue: &wgpu::Queue,
        dst: &wgpu::Buffer,
        bytes: &[u8],
    ) {
        if bytes.is_empty() {
            return;
        }
        let i = match self.last.iter().position(|(l, _, _)| *l == label) {
            Some(i) => i,
            None => {
                self.last.push((label, u64::MAX, Vec::new()));
                self.last.len() - 1
            }
        };
        let (_, g, held) = &mut self.last[i];
        if *g == generation && held.as_slice() == bytes {
            return;
        }
        *g = generation;
        held.clear();
        held.extend_from_slice(bytes);
        self.write(queue, dst, 0, bytes);
    }

    /// Write `bytes` to `dst` at `offset` before the next submitted frame commands. `offset`
    /// must be 4-byte aligned; the size is padded to 4 bytes with zeros, so `dst` must have
    /// room for the padded size (every caller's buffer does: arenas and uniform slots are
    /// allocated in padded units).
    pub(crate) fn write(&mut self, queue: &wgpu::Queue, dst: &wgpu::Buffer, offset: u64, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        const ALIGN: usize = 4;
        const _: () = assert!(wgpu::COPY_BUFFER_ALIGNMENT == 4);
        let size = bytes.len().next_multiple_of(ALIGN);
        let fits = offset.is_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT)
            && offset.checked_add(size as u64).is_some_and(|end| end <= dst.size());
        if !self.batched || !fits {
            if size == bytes.len() {
                queue.write_buffer(dst, offset, bytes);
            } else {
                let mut padded = bytes.to_vec();
                padded.resize(size, 0);
                queue.write_buffer(dst, offset, &padded);
            }
            return;
        }
        let start = self.staging.len();
        self.staging.extend_from_slice(bytes);
        self.staging.resize(start + size, 0);
        self.copies.push(PendingCopy { dst: dst.clone(), dst_offset: offset, src: start..start + size });
    }

    /// Upload everything staged since the last flush with one `write_buffer` and return the
    /// command buffer that copies it into place; submit it in the same `submit` call, BEFORE
    /// the commands that read the data. `None` when nothing is staged.
    pub(crate) fn flush(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        stats: &mut crate::stats::FrameStats,
    ) -> Option<wgpu::CommandBuffer> {
        if self.copies.is_empty() {
            return None;
        }
        let len = self.staging.len() as u64;
        if self.upload.as_ref().is_none_or(|b| b.size() < len) {
            let desc = wgpu::BufferDescriptor {
                label: Some("pebbles.gpu.buffer.upload"),
                size: len.next_power_of_two().max(64 * 1024),
                usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            };
            // The old buffer is dropped first: it frees what the new one needs.
            self.upload = None;
            self.upload =
                crate::execute::alloc::create_with_retry(device, || device.create_buffer(&desc), || {});
            stats.buffers_created += u32::from(self.upload.is_some());
        }
        let Some(upload) = self.upload.as_ref() else {
            // No upload buffer (out of memory): write each region directly, as before M33.
            for c in self.copies.drain(..) {
                queue.write_buffer(&c.dst, c.dst_offset, self.staging.get(c.src).unwrap_or_default());
            }
            self.staging.clear();
            return None;
        };
        queue.write_buffer(upload, 0, &self.staging);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("pebbles.gpu.encoder.upload"),
        });
        for c in self.copies.drain(..) {
            let (src, size) = (c.src.start as u64, c.src.len() as u64);
            encoder.copy_buffer_to_buffer(upload, src, &c.dst, c.dst_offset, size);
        }
        self.staging.clear();
        Some(encoder.finish())
    }
}
