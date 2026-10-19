//! Gradient draws (09 §5.3–§5.5, L35): resolve every `GradientDraw`'s ramp in the LUT, write
//! its `GradientParams` slot, and keep the group-1 bind group (LUT view + params buffer).

use super::pipelines::{PipelineKey, PipelineKind};
use crate::gpu_types::GradientParams;
use crate::renderer::Renderer;
use crate::stats::{FrameStats, Unsupported};

/// Per-renderer gradient resources: the params uniform arena and its bind group.
pub(crate) struct GradientResources {
    pub lut: super::resources::gradient_lut::GradientLut,
    buffer: wgpu::Buffer,
    slots: u64,
    buffer_generation: u64,
    /// Keyed by (LUT generation, buffer generation).
    group: Option<((u64, u64), wgpu::BindGroup)>,
}

fn params_buffer(device: &wgpu::Device, slots: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("pebbles.gpu.buffer.gradients"),
        size: slots * super::tier_c::slot_stride(device),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl GradientResources {
    pub(crate) fn new(device: &wgpu::Device, max_rows: u32) -> Self {
        Self {
            lut: super::resources::gradient_lut::GradientLut::new(device, max_rows),
            buffer: params_buffer(device, 1),
            slots: 1,
            buffer_generation: 0,
            group: None,
        }
    }

    pub(crate) fn bind_group(&self) -> Option<&wgpu::BindGroup> {
        self.group.as_ref().map(|(_, g)| g)
    }

    /// Dynamic offset of gradient `i`'s params.
    pub(crate) fn offset(device: &wgpu::Device, i: u32) -> u32 {
        u32::try_from(u64::from(i) * super::tier_c::slot_stride(device)).unwrap_or(0)
    }
}

/// Before the passes: LUT placement + params for every gradient of the plan; pipelines for the
/// variants in use (Shape/Mask in the single-sampled passes, Mesh at the Tier C sample count).
pub(crate) fn prepare(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    stats: &mut FrameStats,
) {
    let n = renderer.scratch.plan.gradients.len() as u64;
    if n == 0 {
        return;
    }
    let res = &mut renderer.gradients;
    if n > res.slots {
        res.slots = n.next_power_of_two();
        res.buffer = params_buffer(device, res.slots);
        res.buffer_generation += 1;
        stats.buffers_created += 1;
    }
    let stride = super::tier_c::slot_stride(device);
    for (i, d) in renderer.scratch.plan.gradients.iter().enumerate() {
        let placed = res.lut.resolve(device, queue, &d.ramp, |out| d.ramp.bake(out), stats);
        let g = &d.geom;
        let params = match placed {
            Some((start, len)) => GradientParams {
                m0: g.m0,
                m1: g.m1,
                info: [g.kind, g.extend, start, len],
                p0: g.p0,
                p1: g.p1,
            },
            None => {
                // §5.3 "Full": the first stop as a solid (kind 5).
                stats.record(Unsupported::LutFull);
                GradientParams { m0: g.m0, m1: g.m1, info: [5, 0, 0, 1], p0: g.p0, p1: d.first }
            }
        };
        queue.write_buffer(&res.buffer, i as u64 * stride, bytemuck::bytes_of(&params));
    }
    let (view, lut_gen) = res.lut.view();
    let key = (lut_gen, res.buffer_generation);
    if res.group.as_ref().is_none_or(|(k, _)| *k != key) {
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pebbles.gpu.bind.gradient"),
            layout: &renderer.layouts.gradient,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(view) },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &res.buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(80),
                    }),
                },
            ],
        });
        stats.bind_groups_created += 1;
        res.group = Some((key, group));
    }
    let mesh_samples = renderer.sample_count;
    for (variant, samples) in [(0u8, 1u32), (1, 1), (2, mesh_samples)] {
        let key = PipelineKey {
            kind: PipelineKind::Gradient,
            color_format: format,
            sample_count: samples,
            blend: 0,
            variant,
        };
        renderer.pipelines.ensure(device, &renderer.shaders, &renderer.layouts, key, stats);
    }
}
