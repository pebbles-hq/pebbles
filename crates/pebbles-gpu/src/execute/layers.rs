//! Offscreen layer targets (09 §3.5–§3.7, L13): one pooled texture per `CompositePath::Offscreen`
//! layer, its Globals slot (`1 + i`) and its composite params slot (`i`).

use super::encode::SegmentTarget;
use crate::frame_targets::FrameTargets;
use crate::gpu_types::CompositeParams;
use crate::prepare::plan::{DrawCmd, TargetRef};
use crate::renderer::Renderer;
use crate::stats::FrameStats;

/// One acquired layer target.
pub(crate) struct LayerTarget {
    pub slot: usize,
    pub segment: SegmentTarget,
    /// Composite bind group and params offset.
    pub composite: (wgpu::BindGroup, u32),
}

/// Acquire and describe every offscreen layer of the plan. Call after `tier_c::encode_runs` so
/// the slot growth below never discards slots written there.
pub(crate) fn acquire_layers(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    targets: &mut FrameTargets,
    format: wgpu::TextureFormat,
    stats: &mut FrameStats,
) -> Vec<Option<LayerTarget>> {
    let n = renderer.scratch.plan.layers.len();
    if n == 0 {
        return Vec::new();
    }
    renderer.ensure_uniform_slots(device, n as u64 + 1, n as u64, stats);
    let stride = super::tier_c::slot_stride(device);
    // Layer content draws with the root pass's pipelines: single-sample, analytic AA (02 §6).
    let samples = 1;
    // Layer i's target lives from its PushLayer to its Composite; a layer whose composite is
    // already encoded hands its target back so siblings share one (09 §3.8).
    let mut span = vec![(usize::MAX, usize::MAX); n];
    for (k, cmd) in renderer.scratch.plan.cmds.iter().enumerate() {
        match cmd {
            DrawCmd::PushLayer { layer } => {
                if let Some(s) = span.get_mut(*layer as usize) {
                    s.0 = k;
                }
            }
            DrawCmd::Composite { layer, .. } => {
                if let Some(s) = span.get_mut(*layer as usize) {
                    s.1 = k;
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::with_capacity(n);
    let mut live: Vec<(usize, usize)> = Vec::new();
    for (i, &(start, end)) in span.iter().enumerate() {
        live.retain(|&(end, slot)| {
            let done = end < start;
            if done {
                targets.pool.release(slot);
            }
            !done
        });
        let desc = renderer.scratch.plan.layers[i];
        let b = desc.bounds;
        let (w, h) = (b.x1.saturating_sub(b.x0), b.y1.saturating_sub(b.y0));
        if w == 0 || h == 0 {
            out.push(None);
            continue;
        }
        let slot = targets.pool.acquire(device, (w, h), format, samples, stats);
        let size = targets.pool.book.slots[slot].size;
        let globals = crate::execute::resources::globals::Globals {
            target_size: [size.0 as f32, size.1 as f32],
            target_origin: [b.x0 as f32, b.y0 as f32],
            inv_target_size: [1.0 / size.0 as f32, 1.0 / size.1 as f32],
            color_flags: renderer.color_flags(),
            _pad0: 0,
        };
        let globals_offset = (i as u64 + 1) * stride;
        queue.write_buffer(renderer.globals_buffer(), globals_offset, bytemuck::bytes_of(&globals));
        let params = CompositeParams {
            bounds: [b.x0 as f32, b.y0 as f32, b.x1 as f32, b.y1 as f32],
            alpha: desc.alpha,
            mix_mode: desc.mix as u8 as u32,
            compose: desc.compose as u8 as u32,
            _pad: 0,
        };
        let params_offset = i as u64 * stride;
        queue.write_buffer(renderer.composite_buffer(), params_offset, bytemuck::bytes_of(&params));
        let (layout, params_buf, generation) =
            (&renderer.layouts.composite, renderer.composite_buffer(), renderer.composite_generation());
        let tex = targets.pool.textures_mut(slot);
        let view = &tex.color.1;
        let group = super::resources::bind_groups::refresh(
            &mut tex.composite_bg,
            generation,
            || {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("pebbles.gpu.bind.composite"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                buffer: params_buf,
                                offset: 0,
                                size: wgpu::BufferSize::new(32),
                            }),
                        },
                    ],
                })
            },
            stats,
        )
        .clone();
        live.push((end, slot));
        out.push(Some(LayerTarget {
            slot,
            segment: SegmentTarget {
                globals_offset: u32::try_from(globals_offset).unwrap_or(0),
                origin: (b.x0, b.y0),
                size,
            },
            composite: (group, u32::try_from(params_offset).unwrap_or(0)),
        }));
    }
    out
}

/// The segment target of `target` (root = Globals slot 0 at the origin).
pub(crate) fn segment_target(
    target: TargetRef,
    root_size: (u32, u32),
    layers: &[Option<LayerTarget>],
) -> Option<SegmentTarget> {
    match target {
        TargetRef::Root => Some(SegmentTarget { globals_offset: 0, origin: (0, 0), size: root_size }),
        TargetRef::Layer(i) => layers.get(i as usize)?.as_ref().map(|l| l.segment),
    }
}
