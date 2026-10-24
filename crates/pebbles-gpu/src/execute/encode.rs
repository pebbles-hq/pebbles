//! The single ordered encode pass: iterate `plan.cmds` and draw each command in painter's order.

use super::super::prepare::{DrawCmd, ScissorRect};
use super::super::renderer::Renderer;
use super::pipelines::{PipelineKey, PipelineKind};
use super::resolve::resolve_glyphs;
use crate::stats::{FrameStats, Unsupported};

/// Tracks the render pass's current scissor so redundant `set_scissor_rect` calls are elided.
struct PassState {
    scissor: Option<(u32, u32, u32, u32)>,
    scissor_changes: u32,
    /// Address of the bound pipeline (identity only, never dereferenced).
    pipeline: Option<*const wgpu::RenderPipeline>,
    pipeline_switches: u32,
    /// M33: addresses of the bound vertex (slot 0) and index buffers (identity only).
    vertex: Option<*const wgpu::Buffer>,
    index: Option<*const wgpu::Buffer>,
    /// Device px of the target's (0, 0).
    origin: (u32, u32),
    /// Target size (scissors clamp to it).
    size: (u32, u32),
    /// M30: a partial frame — every scissor is intersected with this device rect.
    damage: Option<ScissorRect>,
}

impl PassState {
    /// Bind `pipeline`, counting a switch when it differs from the bound one (10 §9.1).
    fn set_pipeline(&mut self, pass: &mut wgpu::RenderPass<'_>, pipeline: &wgpu::RenderPipeline) {
        let id: *const wgpu::RenderPipeline = pipeline;
        // M33: rebinding the bound pipeline is skipped (the driver re-emits its state).
        if self.pipeline != Some(id) {
            self.pipeline = Some(id);
            self.pipeline_switches += 1;
            pass.set_pipeline(pipeline);
        }
    }

    /// M33: bind `buffer` (whole) as vertex buffer 0 unless it already is; vertex and index
    /// bindings persist across pipeline changes.
    fn set_vertex_buffer(&mut self, pass: &mut wgpu::RenderPass<'_>, buffer: &wgpu::Buffer) {
        let id: *const wgpu::Buffer = buffer;
        if self.vertex != Some(id) {
            self.vertex = Some(id);
            pass.set_vertex_buffer(0, buffer.slice(..));
        }
    }

    /// M33: bind `buffer` (whole, `u32`) as the index buffer unless it already is.
    fn set_index_buffer(&mut self, pass: &mut wgpu::RenderPass<'_>, buffer: &wgpu::Buffer) {
        let id: *const wgpu::Buffer = buffer;
        if self.index != Some(id) {
            self.index = Some(id);
            pass.set_index_buffer(buffer.slice(..), wgpu::IndexFormat::Uint32);
        }
    }

    fn new() -> Self {
        Self {
            scissor: None,
            scissor_changes: 0,
            pipeline: None,
            pipeline_switches: 0,
            vertex: None,
            index: None,
            origin: (0, 0),
            size: (u32::MAX, u32::MAX),
            damage: None,
        }
    }

    /// Set the scissor in the current target's space (device px − target origin, clamped to
    /// the target size); false when it is empty there (the draw must be skipped, 09 §3.6).
    fn set_scissor(&mut self, pass: &mut wgpu::RenderPass<'_>, scissor: ScissorRect) -> bool {
        let scissor = match self.damage {
            Some(d) => ScissorRect {
                x0: scissor.x0.max(d.x0),
                y0: scissor.y0.max(d.y0),
                x1: scissor.x1.min(d.x1),
                y1: scissor.y1.min(d.y1),
            },
            None => scissor,
        };
        let (ox, oy) = self.origin;
        let (w, h) = self.size;
        let x0 = scissor.x0.saturating_sub(ox).min(w);
        let y0 = scissor.y0.saturating_sub(oy).min(h);
        let x1 = scissor.x1.saturating_sub(ox).min(w);
        let y1 = scissor.y1.saturating_sub(oy).min(h);
        if x1 <= x0 || y1 <= y0 {
            return false;
        }
        let rect = (x0, y0, x1 - x0, y1 - y0);
        if self.scissor != Some(rect) {
            self.scissor_changes += 1;
            pass.insert_debug_marker("pebbles.gpu.clip.scissor");
            pass.set_scissor_rect(rect.0, rect.1, rect.2, rect.3);
            self.scissor = Some(rect);
        }
        true
    }
}

/// Resolve glyphs and images for the whole frame (10 §2); the arenas are uploaded per
/// segment by `Renderer::upload_segment` (M07).
pub(crate) fn resolve_frame(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    stats: &mut FrameStats,
) {
    resolve_glyphs(&mut renderer.glyph_atlas, &renderer.scratch.plan, &mut renderer.glyph_instances, stats);
    super::resolve::cull_resolved(&renderer.scratch.plan, &mut renderer.glyph_instances);
    // 09 L08: mask clips live in the coverage atlas too (staged before the flush).
    renderer.resolve_mask_clips(stats);
    // SYNC (10 §5.7): grow (separate submit) before this frame's uploads, then refresh the bind group.
    renderer.glyph_atlas.flush(device, queue, &renderer.layouts.glyph, stats);
    renderer.write_mask_clips(device, queue, stats);
    renderer.image_instances.clear();
    for req in &renderer.scratch.plan.images {
        let drawable = match &req.dynamic {
            Some(frame) => renderer.image_cache.ensure_dynamic(device, queue, req.key, frame, stats),
            None => renderer.image_cache.ensure(device, queue, &req.image, req.mips, stats),
        };
        if drawable {
            let (layout, samplers) = (&renderer.layouts.texture, &mut renderer.samplers);
            renderer.image_cache.ensure_bind_group(device, layout, &req.key, req.sampler, samplers, stats);
        }
        // An image that cannot be drawn becomes a zero-area quad so ranges stay valid.
        let mut quad = req.quad;
        // The texture's own last level (a downscaled or single-level texture has fewer than
        // prepare assumed): `sample_portable` loads levels up to it only (DR-21).
        #[allow(clippy::cast_precision_loss)] // ≤ 31
        if quad.layer_kind[1] != 3 {
            quad.color[1] = quad.color[1].min(renderer.image_cache.top_level(&req.key) as f32);
        }
        renderer.image_instances.push(if drawable { quad } else { bytemuck::Zeroable::zeroed() });
    }
    // R61: GPU-texture quads follow the uploaded images; an unknown or unregistered id becomes
    // a zero-area quad and is counted.
    for req in &renderer.scratch.plan.gpu_images {
        let (layout, samplers) = (&renderer.layouts.texture, &mut renderer.samplers);
        let ok = renderer.external_textures.ensure_bind_group(
            device,
            layout,
            req.id,
            req.sampler,
            samplers,
            stats,
        );
        if !ok {
            stats.record(Unsupported::GpuTextureUnregistered);
        }
        renderer.image_instances.push(if ok { req.quad } else { bytemuck::Zeroable::zeroed() });
    }
    // R63: mesh textures (the image cache) and vertices.
    for (image, key, sampler) in &renderer.scratch.plan.mesh_images {
        if renderer.image_cache.ensure(device, queue, image, false, stats) {
            let (layout, samplers) = (&renderer.layouts.texture, &mut renderer.samplers);
            renderer.image_cache.ensure_bind_group(device, layout, key, *sampler, samplers, stats);
        }
    }
}

/// Draw every command of the renderer's current plan, in order, into `pass`.
/// The pipeline family a draw command uses.
pub(crate) fn cmd_kind(cmd: &DrawCmd) -> PipelineKind {
    match cmd {
        DrawCmd::Solid { .. } => PipelineKind::Solid,
        DrawCmd::Shapes { .. } => PipelineKind::Sdf,
        DrawCmd::Shadows { .. } => PipelineKind::Shadow,
        DrawCmd::Glyphs { .. } => PipelineKind::Glyph,
        DrawCmd::Images { .. } | DrawCmd::GpuImage { .. } => PipelineKind::Image,
        DrawCmd::Vertices { .. } => PipelineKind::Vertices,
        DrawCmd::Brush { paint: crate::prepare::plan::BrushPaint::Gradient(_), .. } => PipelineKind::Gradient,
        DrawCmd::Brush { .. } => PipelineKind::ImageBrush,
        // Layer boundaries and custom ops never reach a pass (09 §3.6, R60); composites draw
        // with CompositeOver.
        DrawCmd::PushLayer { .. }
        | DrawCmd::PopLayer { .. }
        | DrawCmd::Composite { .. }
        | DrawCmd::Custom { .. } => PipelineKind::CompositeOver,
    }
}

/// K2: ensure every pipeline the main pass of `plan` will bind (before any pass opens).
pub(crate) fn ensure_plan_pipelines(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    composites: bool,
    stats: &mut FrameStats,
) {
    let mut seen = [false; PipelineKind::ALL.len()];
    let kinds = renderer
        .scratch
        .plan
        .cmds
        .iter()
        .map(cmd_kind)
        .chain(composites.then_some(PipelineKind::CompositeOver));
    // Brush kinds are ensured per variant by their own prepare steps.
    for kind in kinds.filter(|k| !matches!(k, PipelineKind::Gradient | PipelineKind::ImageBrush)) {
        let i = PipelineKind::ALL.iter().position(|k| *k == kind).unwrap_or(0);
        if std::mem::replace(&mut seen[i], true) {
            continue;
        }
        let key = PipelineKey { kind, color_format: format, sample_count: 1, blend: 0, variant: 0 };
        renderer.pipelines.ensure(device, &renderer.shaders, &renderer.layouts, key, stats);
    }
}

/// Maximal runs of consecutive same-kind draw commands with their debug labels (S52).
pub(crate) fn debug_group_runs(cmds: &[DrawCmd]) -> Vec<(std::ops::Range<usize>, &'static str)> {
    let mut out: Vec<(std::ops::Range<usize>, &'static str)> = Vec::new();
    for (i, cmd) in cmds.iter().enumerate() {
        let label = cmd_kind(cmd).debug_label();
        match out.last_mut() {
            Some((r, l)) if *l == label && r.end == i => r.end = i + 1,
            _ => out.push((i..i + 1, label)),
        }
    }
    out
}

/// The target a pass segment draws into (09 §3.5–§3.6).
#[derive(Clone, Copy, Debug)]
pub(crate) struct SegmentTarget {
    /// Byte offset of the target's Globals slot.
    pub globals_offset: u32,
    /// Device px of the target's (0, 0).
    pub origin: (u32, u32),
    /// Target texture size.
    pub size: (u32, u32),
}

/// Draw the commands `range` of the renderer's plan, in order, into `pass` (one segment).
/// `layers[i]` = layer `i`'s composite bind group and params offset (09 §3.7).
#[allow(clippy::too_many_arguments)]
pub(crate) fn encode(
    renderer: &Renderer,
    pass: &mut wgpu::RenderPass<'_>,
    format: wgpu::TextureFormat,
    stats: &mut FrameStats,
    composites: &[super::tier_c::Composite],
    range: std::ops::Range<usize>,
    target: SegmentTarget,
    layers: &[Option<super::layers::LayerTarget>],
    damage: Option<ScissorRect>,
) {
    // Group 0: Globals slot 0 + the NONE clips; rebound when a cmd's SDF or mask clip changes
    // (09 §2.4, L08).
    let mut bound = (crate::prepare::SdfClipId::NONE, crate::prepare::plan::MaskClipId::NONE);
    let g = target.globals_offset;
    pass.set_bind_group(0, renderer.globals_bg(), &[g, 0, 0]);
    stats.bind_group_switches += 1;
    let mut bind_clip =
        |pass: &mut wgpu::RenderPass<'_>, clip: crate::prepare::ClipState, stats: &mut FrameStats| {
            if (clip.sdf, clip.mask) != bound {
                let offsets = [g, renderer.clip_offset(clip.sdf), renderer.mask_clip_offset(clip.mask)];
                pass.set_bind_group(0, renderer.globals_bg_for(clip.mask), &offsets);
                stats.bind_group_switches += 1;
                bound = (clip.sdf, clip.mask);
            }
        };
    // The main pass is single-sampled (R17).
    let sample_count = 1;
    // M07: the arenas hold the current segment's slices.
    let sb = renderer.seg_base;
    use crate::prepare::segments::{GLYPHS, IMAGES, INDICES, MESHES, SHADOWS, SHAPES};
    let key = |kind, variant| PipelineKey { kind, color_format: format, sample_count, blend: 0, variant };
    let mut state = PassState::new();
    state.origin = target.origin;
    state.size = target.size;
    // M30: confine the whole pass to the damage, including draws that keep the pass scissor.
    state.damage = damage;
    if damage.is_some() {
        let (ox, oy) = state.origin;
        let full = ScissorRect { x0: ox, y0: oy, x1: ox + state.size.0, y1: oy + state.size.1 };
        if !state.set_scissor(pass, full) {
            return;
        }
    }

    let mut runs = composites.iter().filter(|c| range.contains(&c.run.cmds.start)).peekable();
    // Debug groups (S52): one per maximal run of same-kind draws; balanced within the pass.
    let groups = debug_group_runs(&renderer.scratch.plan.cmds);
    let (mut g, mut open) = (0, false);
    let mut i = range.start;
    while i < range.end
        && let Some(cmd) = renderer.scratch.plan.cmds.get(i)
    {
        if open && groups.get(g).is_some_and(|(r, _)| i >= r.end) {
            pass.pop_debug_group();
            open = false;
        }
        while groups.get(g).is_some_and(|(r, _)| r.end <= i) {
            g += 1;
        }
        if !open && let Some((r, label)) = groups.get(g).filter(|(r, _)| r.contains(&i)) {
            let _ = r;
            pass.push_debug_group(label);
            open = true;
        }
        i += 1;
        // A Tier C run is drawn as its composite at the position of its first cmd (R17).
        if let Some(c) = runs.next_if(|c| c.run.cmds.start == i - 1) {
            i = c.run.cmds.end;
            let (Some(pipeline), Some(bg)) =
                (renderer.pipelines.get_ensured(key(PipelineKind::CompositeOver, 0)), &c.bind_group)
            else {
                continue;
            };
            if !state.set_scissor(pass, c.run.scissor) {
                continue;
            }
            bind_clip(pass, c.run.clip, stats);
            state.set_pipeline(pass, pipeline);
            pass.set_bind_group(1, bg, &[c.params_offset]);
            stats.bind_group_switches += 1;
            pass.draw(0..4, 0..1);
            stats.draw_calls += 1;
            continue;
        }
        match cmd {
            DrawCmd::Solid { indices, clip } => {
                let (Some(pipeline), Some(vb), Some(ib)) = (
                    renderer.pipelines.get_ensured(key(PipelineKind::Solid, 0)),
                    renderer.arenas.solid_vertices.buffer(),
                    renderer.arenas.solid_indices.buffer(),
                ) else {
                    continue;
                };
                state.set_pipeline(pass, pipeline);
                state.set_vertex_buffer(pass, vb);
                state.set_index_buffer(pass, ib);
                if !state.set_scissor(pass, clip.scissor) {
                    continue;
                }
                bind_clip(pass, *clip, stats);
                let range = sb.rebase(INDICES, indices);
                crate::execute::pipelines::draw_indexed_portable(pass, range, 0..1);
                stats.draw_calls += 1;
            }
            DrawCmd::Shapes { instances, clip } => {
                let (Some(pipeline), Some(vb)) = (
                    renderer.pipelines.get_ensured(key(PipelineKind::Sdf, 0)),
                    renderer.arenas.shape_instances.buffer(),
                ) else {
                    continue;
                };
                state.set_pipeline(pass, pipeline);
                state.set_vertex_buffer(pass, vb);
                if !state.set_scissor(pass, clip.scissor) {
                    continue;
                }
                bind_clip(pass, *clip, stats);
                pass.draw(0..4, sb.rebase(SHAPES, instances));
                stats.draw_calls += 1;
            }
            DrawCmd::Shadows { instances, clip } => {
                let (Some(pipeline), Some(vb)) = (
                    renderer.pipelines.get_ensured(key(PipelineKind::Shadow, 0)),
                    renderer.arenas.shadow_instances.buffer(),
                ) else {
                    continue;
                };
                state.set_pipeline(pass, pipeline);
                state.set_vertex_buffer(pass, vb);
                if !state.set_scissor(pass, clip.scissor) {
                    continue;
                }
                bind_clip(pass, *clip, stats);
                pass.draw(0..4, sb.rebase(SHADOWS, instances));
                stats.draw_calls += 1;
            }
            DrawCmd::Glyphs { glyphs, clip } => {
                let (Some(pipeline), Some(vb), Some(bg)) = (
                    renderer.pipelines.get_ensured(key(PipelineKind::Glyph, 0)),
                    renderer.arenas.glyphs.buffer(),
                    renderer.glyph_atlas.bind_group(),
                ) else {
                    continue;
                };
                if !state.set_scissor(pass, clip.scissor) {
                    continue;
                }
                bind_clip(pass, *clip, stats);
                state.set_pipeline(pass, pipeline);
                pass.set_bind_group(1, bg, &[]);
                stats.bind_group_switches += 1;
                state.set_vertex_buffer(pass, vb);
                pass.draw(0..4, sb.rebase(GLYPHS, glyphs));
                stats.draw_calls += 1;
            }
            DrawCmd::Images { images, clip } => {
                let first = renderer.scratch.plan.images.get(images.start as usize);
                let (Some(pipeline), Some(vb), Some(bg)) = (
                    renderer.pipelines.get_ensured(key(PipelineKind::Image, 0)),
                    renderer.arenas.images.buffer(),
                    first.and_then(|r| renderer.image_cache.bind_group(&r.key, r.sampler)),
                ) else {
                    continue;
                };
                if !state.set_scissor(pass, clip.scissor) {
                    continue;
                }
                bind_clip(pass, *clip, stats);
                state.set_pipeline(pass, pipeline);
                pass.set_bind_group(1, bg, &[]);
                stats.bind_group_switches += 1;
                state.set_vertex_buffer(pass, vb);
                pass.draw(0..4, sb.rebase(IMAGES, images));
                stats.draw_calls += 1;
            }
            DrawCmd::GpuImage { image, clip } => {
                let Some(req) = renderer.scratch.plan.gpu_images.get(*image as usize) else { continue };
                let (Some(pipeline), Some(vb), Some(bg)) = (
                    renderer.pipelines.get_ensured(key(PipelineKind::Image, 0)),
                    renderer.arenas.images.buffer(),
                    renderer.external_textures.bind_group(req.id, req.sampler),
                ) else {
                    continue;
                };
                if !state.set_scissor(pass, clip.scissor) {
                    continue;
                }
                bind_clip(pass, *clip, stats);
                state.set_pipeline(pass, pipeline);
                pass.set_bind_group(1, bg, &[]);
                stats.bind_group_switches += 1;
                state.set_vertex_buffer(pass, vb);
                let i = sb.gpu_instance(*image);
                pass.draw(0..4, i..i + 1);
                stats.draw_calls += 1;
            }
            DrawCmd::Composite { layer, clip } => {
                let Some(l) = layers.get(*layer as usize).and_then(Option::as_ref) else { continue };
                let desc = &renderer.scratch.plan.layers[*layer as usize];
                let Some(pipeline) =
                    renderer.pipelines.get_ensured(super::layers::composite_key(desc, format))
                else {
                    continue;
                };
                // A projected layer lands anywhere under its parent clip (09 §10.7).
                let b = if desc.path == crate::prepare::plan::CompositePath::Projected {
                    clip.scissor
                } else {
                    desc.bounds
                };
                let s = clip.scissor;
                let scissor = ScissorRect {
                    x0: s.x0.max(b.x0),
                    y0: s.y0.max(b.y0),
                    x1: s.x1.min(b.x1),
                    y1: s.y1.min(b.y1),
                };
                if scissor.x1 <= scissor.x0 || scissor.y1 <= scissor.y0 || !state.set_scissor(pass, scissor) {
                    continue;
                }
                bind_clip(pass, *clip, stats);
                state.set_pipeline(pass, pipeline);
                pass.set_bind_group(1, &l.composite.0, &[l.composite.1]);
                stats.bind_group_switches += 1;
                pass.draw(0..4, 0..1);
                stats.draw_calls += 1;
            }
            DrawCmd::Brush { cover, paint, clip } => {
                use crate::prepare::plan::BrushCover;
                // Mesh covers draw inside their Tier C run (composited above).
                let (variant, range, vb) = match cover {
                    BrushCover::Shape(r) => {
                        (0, sb.rebase(SHAPES, r), renderer.arenas.shape_instances.buffer())
                    }
                    BrushCover::Mask(r) => (1, sb.rebase(GLYPHS, r), renderer.arenas.glyphs.buffer()),
                    BrushCover::Mesh(_) => continue,
                };
                let (Some((pipeline, bg, offset)), Some(vb)) =
                    (renderer.brush_draw(*paint, variant, format, sample_count), vb)
                else {
                    continue;
                };
                if !state.set_scissor(pass, clip.scissor) {
                    continue;
                }
                bind_clip(pass, *clip, stats);
                state.set_pipeline(pass, pipeline);
                pass.set_bind_group(1, bg, &[offset]);
                stats.bind_group_switches += 1;
                state.set_vertex_buffer(pass, vb);
                pass.draw(0..4, range);
                stats.draw_calls += 1;
            }
            DrawCmd::Vertices { vertices, image, clip } => {
                let bg = match image.and_then(|i| renderer.scratch.plan.mesh_images.get(i as usize)) {
                    Some((_, k, s)) => renderer.image_cache.bind_group(k, *s),
                    None => Some(renderer.white_bind_group()),
                };
                let (Some(pipeline), Some(vb), Some(bg)) = (
                    renderer.pipelines.get_ensured(key(PipelineKind::Vertices, 0)),
                    renderer.arenas.meshes.buffer(),
                    bg,
                ) else {
                    continue;
                };
                if !state.set_scissor(pass, clip.scissor) {
                    continue;
                }
                bind_clip(pass, *clip, stats);
                state.set_pipeline(pass, pipeline);
                pass.set_bind_group(1, bg, &[]);
                stats.bind_group_switches += 1;
                state.set_vertex_buffer(pass, vb);
                pass.draw(sb.rebase(MESHES, vertices), 0..1);
                stats.draw_calls += 1;
            }
            // Never inside a segment (09 §3.6).
            DrawCmd::PushLayer { .. } | DrawCmd::PopLayer { .. } | DrawCmd::Custom { .. } => {}
        }
    }
    if open {
        pass.pop_debug_group();
    }
    stats.scissor_changes += state.scissor_changes;
    stats.pipeline_switches += state.pipeline_switches;
    // Leave the pass with a full-target scissor: wgpu's GL backend resolves MSAA with
    // glBlitFramebuffer under the last scissor, which would drop every pixel outside the last
    // clip (Vulkan/Metal/DX12 resolves ignore the scissor).
    let (ox, oy) = state.origin;
    let full = ScissorRect { x0: ox, y0: oy, x1: ox + state.size.0, y1: oy + state.size.1 };
    let _ = state.set_scissor(pass, full);
}

#[cfg(test)]
mod tests {
    use super::{cmd_kind, debug_group_runs};
    use crate::prepare::DrawCmd;

    #[test]
    fn debug_group_runs_partition_commands() {
        let clip = crate::prepare::ClipState::full((8, 8));
        let cmds = vec![
            DrawCmd::Solid { indices: 0..3, clip },
            DrawCmd::Solid { indices: 3..6, clip },
            DrawCmd::Shapes { instances: 0..1, clip },
            DrawCmd::Glyphs { glyphs: 0..2, clip },
            DrawCmd::Glyphs { glyphs: 2..4, clip },
            DrawCmd::Solid { indices: 6..9, clip },
        ];
        let runs = debug_group_runs(&cmds);
        assert_eq!(runs.iter().map(|r| r.0.clone()).collect::<Vec<_>>(), [0..2, 2..3, 3..5, 5..6]);
        assert_eq!(runs[0].0.start, 0);
        assert_eq!(runs.last().map(|r| r.0.end), Some(cmds.len()));
        for w in runs.windows(2) {
            assert_eq!(w[0].0.end, w[1].0.start, "contiguous");
            assert_ne!(w[0].1, w[1].1, "adjacent runs differ in kind");
        }
        for (r, label) in &runs {
            assert!(r.clone().all(|i| cmd_kind(&cmds[i]).debug_label() == *label));
        }
        assert!(debug_group_runs(&[]).is_empty());
    }
}
