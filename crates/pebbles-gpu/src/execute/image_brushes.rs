//! Image-brush draws (09 §6, L42): make each brush image resident (the C13 image cache, a
//! dedicated texture per image), write its `ImageBrushParams` slot and build the group-1 bind
//! group (texture + the R30 sampler + params) for the frame.

use super::pipelines::{PipelineKey, PipelineKind};
use crate::gpu_types::ImageBrushParams;
use crate::renderer::Renderer;
use crate::stats::FrameStats;

pub(crate) struct ImageBrushResources {
    buffer: wgpu::Buffer,
    slots: u64,
    /// Per `ImageBrushDraw` of this frame: its bind group (`None`: not drawable).
    groups: Vec<Option<wgpu::BindGroup>>,
}

fn params_buffer(device: &wgpu::Device, slots: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("pebbles.gpu.buffer.image_brushes"),
        size: slots * super::tier_c::slot_stride(device),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl ImageBrushResources {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        Self { buffer: params_buffer(device, 1), slots: 1, groups: Vec::new() }
    }

    /// Group 1 and the dynamic offset of brush `i`.
    pub(crate) fn binding(&self, device: &wgpu::Device, i: u32) -> Option<(&wgpu::BindGroup, u32)> {
        let group = self.groups.get(i as usize)?.as_ref()?;
        let offset = u32::try_from(u64::from(i) * super::tier_c::slot_stride(device)).ok()?;
        Some((group, offset))
    }

    pub(crate) fn end_frame(&mut self) {
        self.groups.clear();
    }
}

/// Before the passes: residency, params and bind groups for every image brush of the plan.
pub(crate) fn prepare(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    stats: &mut FrameStats,
) {
    renderer.image_brushes.groups.clear();
    let n = renderer.scratch.plan.image_brushes.len() as u64;
    if n == 0 {
        return;
    }
    if n > renderer.image_brushes.slots {
        renderer.image_brushes.slots = n.next_power_of_two();
        renderer.image_brushes.buffer = params_buffer(device, renderer.image_brushes.slots);
        stats.buffers_created += 1;
    }
    let stride = super::tier_c::slot_stride(device);
    for i in 0..renderer.scratch.plan.image_brushes.len() {
        let d = &renderer.scratch.plan.image_brushes[i];
        let mut params: ImageBrushParams = d.params;
        // DR-21: `brush_paint` filters in code by these bits (an exact small integer as f32).
        #[allow(clippy::cast_precision_loss)] // ≤ 63
        {
            params.m0[3] = d.sampler.mode() as f32;
        }
        queue.write_buffer(&renderer.image_brushes.buffer, i as u64 * stride, bytemuck::bytes_of(&params));
        let drawable = renderer.image_cache.ensure(device, queue, &d.image, false, stats);
        let group = match (drawable, renderer.image_cache.view(&d.key)) {
            (true, Some(view)) => {
                let sampler = renderer.samplers.get_or_create(device, d.sampler, stats);
                let g = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("pebbles.gpu.bind.image_brush"),
                    layout: &renderer.layouts.image_brush,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: &renderer.image_brushes.buffer,
                                offset: 0,
                                size: wgpu::BufferSize::new(48),
                            }),
                        },
                    ],
                });
                stats.bind_groups_created += 1;
                Some(g)
            }
            _ => None,
        };
        renderer.image_brushes.groups.push(group);
    }
    let mesh_samples = renderer.sample_count;
    for (variant, samples) in [(0u8, 1u32), (1, 1), (2, mesh_samples)] {
        let key = PipelineKey {
            kind: PipelineKind::ImageBrush,
            color_format: format,
            sample_count: samples,
            blend: 0,
            variant,
        };
        renderer.pipelines.ensure(device, &renderer.shaders, &renderer.layouts, key, stats);
    }
}
