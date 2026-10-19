//! Offscreen layer targets (09 §3.5–§3.7, L13): one pooled texture per `CompositePath::Offscreen`
//! layer, its Globals slot (`1 + i`) and its composite params slot (`i`).

use super::encode::SegmentTarget;
use super::pipelines::{PipelineKey, PipelineKind};
use crate::frame_targets::FrameTargets;
use crate::gpu_types::CompositeParams;
use crate::prepare::plan::{CompositePath, DrawCmd, LayerDesc, TargetRef};
use crate::renderer::Renderer;
use crate::stats::FrameStats;

/// One acquired layer target.
pub(crate) struct LayerTarget {
    pub slot: usize,
    pub segment: SegmentTarget,
    /// Composite bind group and params offset.
    pub composite: (wgpu::BindGroup, u32),
    /// `Backdrop` path (09 §4.4): the pool slot the parent's pixels are copied into.
    pub backdrop: Option<usize>,
    /// Effect layers (09 §10.3): the passes run after the layer's last segment.
    pub effect_passes: Vec<super::effects::EffectPass>,
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
    let mut effect_slot = 0u32;
    let mut all_passes: Vec<super::effects::EffectPass> = Vec::new();
    for path in
        [CompositePath::Over, CompositePath::Compose, CompositePath::Backdrop, CompositePath::Projected]
    {
        for d in renderer.scratch.plan.layers.iter().filter(|d| d.path == path) {
            let key = composite_key(d, format);
            renderer.pipelines.ensure(device, &renderer.shaders, &renderer.layouts, key, stats);
        }
    }
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
        // A projected layer keeps ≥ 1 transparent texel past its content so bilinear sampling
        // at the content edge always blends with transparent (09 §10.7; the golden model's decal).
        let pad = u32::from(desc.path == CompositePath::Projected);
        // 11 §2.6: a layer whose target cannot be created (OOM twice) is dropped this frame.
        let Some(slot) = targets.pool.try_acquire(device, (w + pad, h + pad), format, samples, stats) else {
            out.push(None);
            continue;
        };
        let size = targets.pool.book.slots[slot].size;
        let globals = crate::execute::resources::globals::Globals {
            target_size: [size.0 as f32, size.1 as f32],
            target_origin: [b.x0 as f32, b.y0 as f32],
            inv_target_size: [1.0 / size.0 as f32, 1.0 / size.1 as f32],
            color_flags: renderer.color_flags(),
            _pad0: 0,
        };
        let globals_offset = (i as u64 + 1) * stride;
        renderer.globals_staging.set(u32::try_from(i + 1).unwrap_or(u32::MAX), &globals);
        let params = CompositeParams {
            bounds: [b.x0 as f32, b.y0 as f32, b.x1 as f32, b.y1 as f32],
            alpha: desc.alpha,
            mix_mode: desc.mix as u8 as u32,
            compose: desc.compose as u8 as u32,
            _pad: 0,
        };
        let params_offset = i as u64 * stride;
        queue.write_buffer(renderer.composite_buffer(), params_offset, bytemuck::bytes_of(&params));
        let params_binding = |buffer| {
            wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer,
                offset: 0,
                size: wgpu::BufferSize::new(32),
            })
        };
        if desc.path == CompositePath::Backdrop {
            // The backdrop copy lives exactly as long as the layer target.
            let bslot = targets.pool.acquire(device, (w, h), format, 1, stats);
            live.push((end, bslot));
            // 09 §10.4: a backdrop filter's source is the filtered copy of the backdrop.
            let mut effect_passes = Vec::new();
            let source = match desc.effect.and_then(|e| renderer.scratch.plan.effects.get(e as usize)) {
                Some(steps) => {
                    let mut acquire = |size: (u32, u32)| {
                        let s = targets.pool.acquire(device, size, format, 1, stats);
                        live.push((end, s));
                        s
                    };
                    super::effects::plan_passes(
                        steps,
                        (w, h),
                        bslot,
                        &mut acquire,
                        &mut effect_slot,
                        &mut effect_passes,
                    )
                }
                None => slot,
            };
            all_passes.extend(effect_passes.iter().cloned());
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("pebbles.gpu.bind.composite_backdrop"),
                layout: &renderer.layouts.composite_backdrop,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&targets.pool.textures(source).color.1),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: params_binding(renderer.composite_buffer()),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&targets.pool.textures(bslot).color.1),
                    },
                ],
            });
            stats.bind_groups_created += 1;
            live.push((end, slot));
            out.push(Some(LayerTarget {
                slot,
                segment: SegmentTarget {
                    globals_offset: u32::try_from(globals_offset).unwrap_or(0),
                    origin: (b.x0, b.y0),
                    size,
                },
                composite: (group, u32::try_from(params_offset).unwrap_or(0)),
                backdrop: Some(bslot),
                effect_passes,
            }));
            continue;
        }
        if desc.path == CompositePath::Projected {
            let proj =
                renderer.scratch.plan.projections.iter().find(|(l, _)| *l as usize == i).map(|(_, h)| *h);
            let Some(proj) = proj else {
                out.push(None);
                continue;
            };
            let k = renderer.write_projected_params(device, queue, proj, (w, h), size, stats);
            let sampler_key = crate::execute::resources::image_cache::SamplerKey::of(
                peniko::ImageSampler::new().with_quality(peniko::ImageQuality::Medium),
            );
            let sampler = renderer.samplers.get_or_create(device, sampler_key, stats).clone();
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("pebbles.gpu.bind.projected"),
                layout: &renderer.layouts.projected,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&targets.pool.textures(slot).color.1),
                    },
                    wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: renderer.projected_buffer(),
                            offset: 0,
                            size: wgpu::BufferSize::new(112),
                        }),
                    },
                ],
            });
            stats.bind_groups_created += 1;
            live.push((end, slot));
            out.push(Some(LayerTarget {
                slot,
                segment: SegmentTarget {
                    globals_offset: u32::try_from(globals_offset).unwrap_or(0),
                    origin: (b.x0, b.y0),
                    size,
                },
                composite: (group, k),
                backdrop: None,
                effect_passes: Vec::new(),
            }));
            continue;
        }
        // 09 §10.3: an effect layer composites from its last effect pass's texture.
        let mut effect_passes = Vec::new();
        let csrc = match desc.effect.and_then(|e| renderer.scratch.plan.effects.get(e as usize)) {
            Some(steps) => {
                let mut acquire = |size: (u32, u32)| {
                    let s = targets.pool.acquire(device, size, format, 1, stats);
                    live.push((end, s));
                    s
                };
                super::effects::plan_passes(
                    steps,
                    (w, h),
                    slot,
                    &mut acquire,
                    &mut effect_slot,
                    &mut effect_passes,
                )
            }
            None => slot,
        };
        all_passes.extend(effect_passes.iter().cloned());
        let (layout, params_buf, generation) =
            (&renderer.layouts.composite, renderer.composite_buffer(), renderer.composite_generation());
        let tex = targets.pool.textures_mut(csrc);
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
            backdrop: None,
            effect_passes,
        }));
    }
    super::effects::ensure_pipelines(renderer, device, format, &all_passes, stats);
    renderer.write_effect_params(device, queue, &all_passes, stats);
    out
}

/// The composite pipeline of layer `d` (09 §4.2): `PipelineKey.blend = compose as u8` for the
/// fixed-function Compose path.
pub(crate) fn composite_key(d: &LayerDesc, format: wgpu::TextureFormat) -> PipelineKey {
    let (kind, blend) = match d.path {
        CompositePath::Over => (PipelineKind::CompositeOver, 0),
        CompositePath::Compose => (PipelineKind::CompositeCompose, d.compose as u8),
        CompositePath::Backdrop => (PipelineKind::CompositeBackdrop, 0),
        CompositePath::Projected => (PipelineKind::Projected, 0),
    };
    PipelineKey { kind, color_format: format, sample_count: 1, blend, variant: 0 }
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
