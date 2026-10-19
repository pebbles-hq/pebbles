//! Shader-brush draws (09 §6.1, L43): compile each program on first use, write its
//! `ShaderBrushParams` slot, make its ≤ 2 images resident and build the group-1 bind group.

use crate::gpu_types::ShaderBrushParams;
use crate::renderer::Renderer;
use crate::stats::FrameStats;

pub(crate) struct ShaderBrushResources {
    buffer: wgpu::Buffer,
    slots: u64,
    /// Per `ShaderBrushDraw` of this frame: its bind group (`None`: the program failed).
    groups: Vec<Option<wgpu::BindGroup>>,
    /// 1×1 transparent texture for unbound `t0` / `t1`.
    transparent: (wgpu::Texture, wgpu::TextureView),
}

fn stride(device: &wgpu::Device) -> u64 {
    let align = u64::from(device.limits().min_uniform_buffer_offset_alignment).max(16);
    (size_of::<ShaderBrushParams>() as u64).div_ceil(align) * align
}

fn params_buffer(device: &wgpu::Device, slots: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("pebbles.gpu.buffer.shader_brushes"),
        size: slots * stride(device),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

impl ShaderBrushResources {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pebbles.gpu.texture.shader_brush_transparent"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            texture.as_image_copy(),
            &[0u8; 4],
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: Some(1) },
            wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self { buffer: params_buffer(device, 1), slots: 1, groups: Vec::new(), transparent: (texture, view) }
    }

    /// Group 1 and the dynamic offset of draw `i`.
    pub(crate) fn binding(&self, device: &wgpu::Device, i: u32) -> Option<(&wgpu::BindGroup, u32)> {
        let group = self.groups.get(i as usize)?.as_ref()?;
        let offset = u32::try_from(u64::from(i) * stride(device)).ok()?;
        Some((group, offset))
    }

    pub(crate) fn end_frame(&mut self) {
        self.groups.clear();
    }
}

/// Before the passes: programs, params, images, bind groups and pipelines of every shader brush.
pub(crate) fn prepare(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    stats: &mut FrameStats,
) {
    renderer.shader_brushes.groups.clear();
    let n = renderer.scratch.plan.shader_brushes.len() as u64;
    if n == 0 {
        return;
    }
    if n > renderer.shader_brushes.slots {
        renderer.shader_brushes.slots = n.next_power_of_two();
        renderer.shader_brushes.buffer = params_buffer(device, renderer.shader_brushes.slots);
        stats.buffers_created += 1;
    }
    let stride = stride(device);
    let mesh_samples = renderer.sample_count;
    for i in 0..renderer.scratch.plan.shader_brushes.len() {
        let d = &renderer.scratch.plan.shader_brushes[i];
        if !renderer.shader_programs.ensure(device, &d.program) {
            stats.shader_errors += 1;
            renderer.shader_brushes.groups.push(None);
            continue;
        }
        let id = d.program.id();
        for (variant, samples) in [(0u8, 1u32), (1, 1), (2, mesh_samples)] {
            renderer.shader_programs.ensure_pipeline(
                device,
                &renderer.layouts,
                id,
                (variant, format, samples),
                stats,
            );
        }
        let d = &renderer.scratch.plan.shader_brushes[i];
        queue.write_buffer(&renderer.shader_brushes.buffer, i as u64 * stride, bytemuck::bytes_of(&d.params));
        let mut views = [None, None];
        for (slot, (image, key)) in views.iter_mut().zip(&d.images) {
            if renderer.image_cache.ensure(device, queue, image, false, stats) {
                *slot = Some(*key);
            }
        }
        let tex = |k: Option<crate::execute::resources::image_cache::ImageKey>| {
            k.and_then(|k| renderer.image_cache.view(&k)).unwrap_or(&renderer.shader_brushes.transparent.1)
        };
        let sampler = renderer.samplers.get_or_create(device, d.sampler, stats);
        let g = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pebbles.gpu.bind.shader_brush"),
            layout: &renderer.layouts.shader_brush,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(tex(views[0])),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(tex(views[1])),
                },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(sampler) },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &renderer.shader_brushes.buffer,
                        offset: 0,
                        size: wgpu::BufferSize::new(size_of::<ShaderBrushParams>() as u64),
                    }),
                },
            ],
        });
        stats.bind_groups_created += 1;
        renderer.shader_brushes.groups.push(Some(g));
    }
}
