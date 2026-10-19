//! GPU coverage masks (09 §2.9, L09): before the frame's content passes, each job's clip mesh
//! is drawn with the `Solid` pipeline into a pooled `R8Unorm` target (4× MSAA where R12 allows,
//! resolved). The parent mask is multiplied in by the same draw: the `Solid` fragment shader
//! already scales its output by the bound mask record's coverage, so binding the parent record
//! yields `own · parent` with no extra pass. Content then samples the texture through group 0
//! binding 4 (a per-texture variant of the globals bind group).

use super::pipelines::{PipelineKey, PipelineKind};
use super::resources::layer_pool::LayerPool;
use crate::renderer::Renderer;
use crate::stats::FrameStats;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

/// The mask pool (`mask_pool` in 09 §2.9) plus this frame's per-job group-0 variants.
pub(crate) struct GpuMasks {
    pool: LayerPool,
    /// Bound at binding 4 when no GPU mask is active (never read: the record flag gates it).
    dummy: (wgpu::Texture, wgpu::TextureView),
    /// Job → its group-0 variant, this frame.
    groups: Vec<wgpu::BindGroup>,
}

impl GpuMasks {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let t = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pebbles.gpu.gpu_mask.dummy"),
            size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let v = t.create_view(&wgpu::TextureViewDescriptor::default());
        Self { pool: LayerPool::default(), dummy: (t, v), groups: Vec::new() }
    }

    pub(crate) fn dummy_view(&self) -> &wgpu::TextureView {
        &self.dummy.1
    }

    /// Group 0 carrying job `job`'s mask texture.
    pub(crate) fn bind_group(&self, job: u32) -> Option<&wgpu::BindGroup> {
        self.groups.get(job as usize)
    }

    /// Release this frame's masks; drop idle / over-budget pool textures (09 §2.9).
    pub(crate) fn end_frame(&mut self, budget: u64) {
        self.groups.clear();
        self.pool.end_frame(budget);
    }

    /// Pooled mask textures and their bytes (census).
    #[cfg(debug_assertions)]
    pub(crate) fn census(&self) -> (usize, u64) {
        (self.pool.len(), self.pool.bytes())
    }
}

/// Render every GPU mask job of the plan, in order (parents precede children).
pub(crate) fn render_jobs(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    stats: &mut FrameStats,
) {
    renderer.gpu_masks.groups.clear();
    let n = renderer.scratch.plan.gpu_mask_jobs.len();
    if n == 0 {
        return;
    }
    // Globals slots: root 0, layers 1..=L, then one per job.
    let base = renderer.scratch.plan.layers.len() as u64 + 1;
    renderer.ensure_uniform_slots(device, base + n as u64, 0, stats);
    let stride = super::tier_c::slot_stride(device);
    let samples = renderer.msaa_samples_for(FORMAT);
    let key = PipelineKey {
        kind: PipelineKind::Solid,
        color_format: FORMAT,
        sample_count: samples,
        blend: 0,
        variant: 0,
    };
    renderer.pipelines.ensure(device, &renderer.shaders, &renderer.layouts, key, stats);
    let generation = renderer.globals_bg_generation();
    for j in 0..n {
        let job = renderer.scratch.plan.gpu_mask_jobs[j].clone();
        let r = job.rect;
        let slot = renderer.gpu_masks.pool.acquire(device, (r.width(), r.height()), FORMAT, samples, stats);
        let size = renderer.gpu_masks.pool.book.slots[slot].size;
        let globals = super::resources::globals::Globals {
            target_size: [size.0 as f32, size.1 as f32],
            target_origin: [r.x0 as f32, r.y0 as f32],
            inv_target_size: [1.0 / size.0 as f32, 1.0 / size.1 as f32],
            color_flags: 0,
            _pad0: 0,
        };
        let globals_offset = (base + j as u64) * stride;
        queue.write_buffer(renderer.globals_buffer(), globals_offset, bytemuck::bytes_of(&globals));
        {
            let tex = renderer.gpu_masks.pool.textures(slot);
            let (view, resolve_target, store) = match &tex.msaa {
                Some((_, m)) => (m, Some(&tex.color.1), wgpu::StoreOp::Discard),
                None => (&tex.color.1, None, wgpu::StoreOp::Store),
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pebbles.gpu.pass.gpu_mask"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            stats.render_passes += 1;
            let (Some(pipeline), Some(vb), Some(ib)) = (
                renderer.pipelines.get_ensured(key),
                renderer.arenas.solid_vertices.buffer(),
                renderer.arenas.solid_indices.buffer(),
            ) else {
                continue;
            };
            pass.set_pipeline(pipeline);
            let offsets =
                [u32::try_from(globals_offset).unwrap_or(0), 0, renderer.mask_clip_offset(job.parent)];
            let parent_bg =
                match renderer.scratch.plan.mask_clips.get(job.parent.0 as usize).and_then(|p| p.gpu) {
                    Some(pj) => renderer.gpu_masks.groups.get(pj as usize).unwrap_or(renderer.globals_bg()),
                    None => renderer.globals_bg(),
                };
            pass.set_bind_group(0, parent_bg, &offsets);
            stats.bind_group_switches += 1;
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(job.indices.clone(), 0, 0..1);
            stats.draw_calls += 1;
        }
        stats.gpu_masks += 1;
        stats.gpu_masks_aliased += u32::from(samples == 1);
        // The group-0 variant content uses under this mask (cached per pool texture).
        let group = {
            let tex = renderer.gpu_masks.pool.textures(slot);
            match &tex.composite_bg {
                Some(g) if g.key == generation => g.group.clone(),
                _ => {
                    let g = renderer.globals_bg_with(device, &tex.color.1);
                    stats.bind_groups_created += 1;
                    renderer.gpu_masks.pool.textures_mut(slot).composite_bg =
                        Some(super::resources::bind_groups::GenBindGroup {
                            key: generation,
                            group: g.clone(),
                        });
                    g
                }
            }
        };
        renderer.gpu_masks.groups.push(group);
    }
}
