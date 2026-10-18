//! Tier C runs (02 §5.2, R17): consecutive tessellated draws that share a clip render into a
//! pooled offscreen MSAA texture sized to their device bounds, are resolved, and are composited
//! into the single-sampled main pass in painter's order with `CompositeOver`.

use std::ops::Range;

use super::pipelines::{PipelineKey, PipelineKind};
use crate::frame_targets::FrameTargets;
use crate::gpu_types::CompositeParams;
use crate::prepare::{DrawCmd, FramePlan, ScissorRect};
use crate::renderer::Renderer;
use crate::stats::FrameStats;

/// One Tier C run: `plan.cmds[cmds]` (all `Solid`, one clip), drawn inside `bounds`.
#[derive(Clone, Debug)]
pub(crate) struct Run {
    pub cmds: Range<usize>,
    /// Device px, rounded out, ∩ the clip scissor; empty ⇒ nothing to draw.
    pub bounds: ScissorRect,
    pub scissor: ScissorRect,
}

fn intersect(a: ScissorRect, b: ScissorRect) -> ScissorRect {
    let (x0, y0) = (a.x0.max(b.x0), a.y0.max(b.y0));
    ScissorRect { x0, y0, x1: a.x1.min(b.x1).max(x0), y1: a.y1.min(b.y1).max(y0) }
}

/// Group consecutive `Solid` cmds sharing a clip into runs with their vertex bounds.
pub(crate) fn plan_runs(plan: &FramePlan, out: &mut Vec<Run>) {
    out.clear();
    let mut i = 0;
    while i < plan.cmds.len() {
        let DrawCmd::Solid { clip, .. } = plan.cmds[i] else {
            i += 1;
            continue;
        };
        let start = i;
        let (mut x0, mut y0, mut x1, mut y1) =
            (f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
        while let Some(DrawCmd::Solid { indices, clip: c }) = plan.cmds.get(i) {
            if *c != clip {
                break;
            }
            for &ix in
                plan.solid_indices.get(indices.start as usize..indices.end as usize).unwrap_or_default()
            {
                if let Some(v) = plan.solid_vertices.get(ix as usize) {
                    (x0, y0, x1, y1) =
                        (x0.min(v.pos[0]), y0.min(v.pos[1]), x1.max(v.pos[0]), y1.max(v.pos[1]));
                }
            }
            i += 1;
        }
        let px = |v: f32, max: u32, up: bool| {
            let v = f64::from(v);
            if up { crate::prepare::num::ceil_px(v, max) } else { crate::prepare::num::floor_px(v, max) }
        };
        let (w, h) = plan.target_size;
        let bounds = match (px(x0, w, false), px(y0, h, false), px(x1, w, true), px(y1, h, true)) {
            (Some(x0), Some(y0), Some(x1), Some(y1)) if x0 < x1 && y0 < y1 => {
                intersect(ScissorRect { x0, y0, x1, y1 }, clip.scissor)
            }
            _ => ScissorRect { x0: 0, y0: 0, x1: 0, y1: 0 },
        };
        out.push(Run { cmds: start..i, bounds, scissor: clip.scissor });
    }
}

/// Bytes between dynamic-offset uniform slots.
pub(crate) fn slot_stride(device: &wgpu::Device) -> u64 {
    u64::from(device.limits().min_uniform_buffer_offset_alignment).max(32)
}

/// A run's composite draw for the main pass: the bind group (layer texture + params slot).
pub(crate) struct Composite {
    pub run: Run,
    pub bind_group: Option<wgpu::BindGroup>,
    pub params_offset: u32,
}

/// Render every run of `renderer.scratch.plan` into its pooled target; return the composites
/// in run order. Globals slot `k + 1` and composite slot `k` belong to run `k`.
pub(crate) fn encode_runs(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    encoder: &mut wgpu::CommandEncoder,
    targets: &mut FrameTargets,
    format: wgpu::TextureFormat,
    stats: &mut FrameStats,
) -> Vec<Composite> {
    let mut runs = std::mem::take(&mut renderer.runs);
    plan_runs(&renderer.scratch.plan, &mut runs);
    if runs.is_empty() {
        renderer.runs = runs;
        return Vec::new();
    }
    let samples = renderer.sample_count;
    if samples == 1 {
        stats.tier_c_aliased += u32::try_from(runs.len()).unwrap_or(u32::MAX);
    }
    let stride = slot_stride(device);
    let n = runs.len() as u64;
    renderer.ensure_uniform_slots(device, n + 1, n);
    let solid = PipelineKey {
        kind: PipelineKind::Solid,
        color_format: format,
        sample_count: samples,
        blend: 0,
        variant: 0,
    };
    let over = PipelineKey {
        kind: PipelineKind::CompositeOver,
        color_format: format,
        sample_count: 1,
        blend: 0,
        variant: 0,
    };
    for key in [solid, over] {
        renderer.pipelines.ensure(device, &renderer.shaders, &renderer.layouts, key, stats);
    }
    let mut out = Vec::with_capacity(runs.len());
    for (k, run) in runs.iter().enumerate() {
        let k64 = k as u64;
        let (w, h) = (run.bounds.x1 - run.bounds.x0, run.bounds.y1 - run.bounds.y0);
        let params_offset = u32::try_from(k64 * stride).unwrap_or(0);
        if w == 0 || h == 0 {
            out.push(Composite { run: run.clone(), bind_group: None, params_offset });
            continue;
        }
        let slot = targets.pool.acquire(device, (w, h), format, samples, stats);
        let tex = targets.pool.textures(slot);
        let size = targets.pool.book.slots[slot].size;
        let globals = crate::execute::resources::globals::Globals {
            target_size: [size.0 as f32, size.1 as f32],
            target_origin: [run.bounds.x0 as f32, run.bounds.y0 as f32],
            inv_target_size: [1.0 / size.0 as f32, 1.0 / size.1 as f32],
            color_flags: 0,
            _pad0: 0,
        };
        let globals_offset = (k64 + 1) * stride;
        queue.write_buffer(renderer.globals_buffer(), globals_offset, bytemuck::bytes_of(&globals));
        let b = run.bounds;
        let params = CompositeParams {
            bounds: [b.x0 as f32, b.y0 as f32, b.x1 as f32, b.y1 as f32],
            alpha: 1.0,
            mix_mode: 0,
            compose: 3, // SrcOver
            _pad: 0,
        };
        queue.write_buffer(renderer.composite_buffer(), k64 * stride, bytemuck::bytes_of(&params));
        {
            let (view, resolve_target) = match &tex.msaa {
                Some((_, m)) => (m, Some(&tex.color.1)),
                None => (&tex.color.1, None),
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pebbles.gpu.pass.tier_c"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let (Some(pipeline), Some(vb), Some(ib)) = (
                renderer.pipelines.get(solid),
                renderer.arenas.solid_vertices.buffer(),
                renderer.arenas.solid_indices.buffer(),
            ) else {
                continue;
            };
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, renderer.globals_bg(), &[u32::try_from(globals_offset).unwrap_or(0)]);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            // The run shares one clip: scissor = clip ∩ bounds, in run-texture space.
            let s = intersect(run.scissor, run.bounds);
            pass.set_scissor_rect(s.x0 - b.x0, s.y0 - b.y0, s.x1 - s.x0, s.y1 - s.y0);
            for cmd in &renderer.scratch.plan.cmds[run.cmds.clone()] {
                if let DrawCmd::Solid { indices, .. } = cmd {
                    pass.draw_indexed(indices.clone(), 0, 0..1);
                    stats.draw_calls += 1;
                }
            }
            stats.render_passes += 1;
        }
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pebbles.gpu.bind.composite"),
            layout: &renderer.layouts.composite,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&tex.color.1),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: renderer.composite_buffer(),
                        offset: 0,
                        size: wgpu::BufferSize::new(32),
                    }),
                },
            ],
        });
        out.push(Composite { run: run.clone(), bind_group: Some(bind_group), params_offset });
    }
    renderer.runs = runs;
    out
}
