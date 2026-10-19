//! Opt-in GPU frame time from timestamp queries (10 B4, M22). Enabled by
//! `PEBBLES_GPU_TIMING=1` (`GpuInit::gpu_timing`) on adapters with `TIMESTAMP_QUERY`; absent
//! otherwise (always on WebGL2), and nothing else changes.
//!
//! Two timestamps bracket the frame's passes (an empty pass on the frame target before the
//! first and after the last). The pair is resolved and copied into one of three readback
//! buffers, mapped without blocking; a later frame reads it — so `FrameStats::gpu_us` **lags the
//! frame it measures by 1–3 frames**.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

enum SlotState {
    Free,
    /// The frame's pair was copied in; map after the submit.
    Copied,
    /// `map_async` requested; the flag is set by its callback.
    Mapping(Arc<AtomicBool>),
}

struct Slot {
    buffer: wgpu::Buffer,
    state: SlotState,
}

pub(crate) struct GpuTimer {
    query_set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    slots: [Slot; 3],
    /// The slot this frame copies into (`None`: none free, the frame is not timed).
    current: Option<usize>,
    period_ns: f32,
    /// The latest measurement (kept until a newer one lands).
    last_us: Option<u32>,
}

fn timing_pass<'e>(
    encoder: &'e mut wgpu::CommandEncoder,
    view: &wgpu::TextureView,
    qs: &wgpu::QuerySet,
    begin: bool,
) -> wgpu::RenderPass<'e> {
    let (b, e) = if begin { (Some(0), None) } else { (None, Some(1)) };
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("pebbles.gpu.pass.timing"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: Some(wgpu::RenderPassTimestampWrites {
            query_set: qs,
            beginning_of_pass_write_index: b,
            end_of_pass_write_index: e,
        }),
        occlusion_query_set: None,
        multiview_mask: None,
    })
}

impl GpuTimer {
    /// A timer when the device has `TIMESTAMP_QUERY` (requested only with `gpu_timing`).
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Option<Self> {
        if !device.features().contains(wgpu::Features::TIMESTAMP_QUERY) {
            return None;
        }
        let query_set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("pebbles.gpu.query.timing"),
            ty: wgpu::QueryType::Timestamp,
            count: 2,
        });
        let buf = |label, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: 16,
                usage,
                mapped_at_creation: false,
            })
        };
        let resolve = buf(
            "pebbles.gpu.buffer.timing_resolve",
            wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
        );
        let slot = || Slot {
            buffer: buf(
                "pebbles.gpu.buffer.timing_readback",
                wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            ),
            state: SlotState::Free,
        };
        Some(Self {
            query_set,
            resolve,
            slots: [slot(), slot(), slot()],
            current: None,
            period_ns: queue.get_timestamp_period(),
            last_us: None,
        })
    }

    /// Before the frame's first pass: write the start timestamp (if a readback slot is free).
    pub(crate) fn begin(&mut self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        self.current = self.slots.iter().position(|s| matches!(s.state, SlotState::Free));
        if self.current.is_some() {
            drop(timing_pass(encoder, view, &self.query_set, true));
        }
    }

    /// After the frame's last pass: the end timestamp, resolve, copy into the slot.
    pub(crate) fn end(&mut self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView) {
        let Some(i) = self.current else { return };
        drop(timing_pass(encoder, view, &self.query_set, false));
        encoder.resolve_query_set(&self.query_set, 0..2, &self.resolve, 0);
        if let Some(slot) = self.slots.get_mut(i) {
            encoder.copy_buffer_to_buffer(&self.resolve, 0, &slot.buffer, 0, 16);
            slot.state = SlotState::Copied;
        }
    }

    /// After the submit: request the map of this frame's slot (never waited on).
    pub(crate) fn after_submit(&mut self) {
        let Some(slot) = self.current.take().and_then(|i| self.slots.get_mut(i)) else { return };
        if matches!(slot.state, SlotState::Copied) {
            let done = Arc::new(AtomicBool::new(false));
            let flag = Arc::clone(&done);
            slot.buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |r| flag.store(r.is_ok(), Ordering::Release));
            slot.state = SlotState::Mapping(done);
        }
    }

    /// Read every mapped slot (non-blocking; native polls the device first); the latest
    /// measurement in µs.
    pub(crate) fn poll(&mut self, device: &wgpu::Device) -> Option<u32> {
        #[cfg(not(target_family = "wasm"))]
        let _ = device.poll(wgpu::PollType::Poll);
        #[cfg(target_family = "wasm")]
        let _ = device;
        for slot in &mut self.slots {
            let SlotState::Mapping(done) = &slot.state else { continue };
            if !done.load(Ordering::Acquire) {
                continue;
            }
            {
                let data = slot.buffer.slice(..).get_mapped_range();
                let ts: [u64; 2] = bytemuck::pod_read_unaligned(&data[..16]);
                if ts[1] > ts[0] {
                    #[allow(
                        clippy::cast_precision_loss,
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss
                    )]
                    let us = ((ts[1] - ts[0]) as f64 * f64::from(self.period_ns) / 1000.0) as u32;
                    self.last_us = Some(us);
                }
            }
            slot.buffer.unmap();
            slot.state = SlotState::Free;
        }
        self.last_us
    }
}
