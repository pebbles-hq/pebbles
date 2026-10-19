//! The single ordered encode pass: iterate `plan.cmds` and draw each command in painter's order.

use super::super::prepare::{DrawCmd, ScissorRect};
use super::super::renderer::Renderer;
use super::pipelines::{PipelineKey, PipelineKind};
use super::resolve::resolve_glyphs;
use crate::stats::FrameStats;

/// Tracks the render pass's current scissor so redundant `set_scissor_rect` calls are elided.
struct PassState {
    scissor: Option<(u32, u32, u32, u32)>,
    scissor_changes: u32,
    /// Address of the bound pipeline (identity only, never dereferenced).
    pipeline: Option<*const wgpu::RenderPipeline>,
    pipeline_switches: u32,
}

impl PassState {
    /// Bind `pipeline`, counting a switch when it differs from the bound one (10 §9.1).
    fn set_pipeline(&mut self, pass: &mut wgpu::RenderPass<'_>, pipeline: &wgpu::RenderPipeline) {
        let id: *const wgpu::RenderPipeline = pipeline;
        if self.pipeline != Some(id) {
            self.pipeline = Some(id);
            self.pipeline_switches += 1;
        }
        pass.set_pipeline(pipeline);
    }

    fn new() -> Self {
        Self { scissor: None, scissor_changes: 0, pipeline: None, pipeline_switches: 0 }
    }

    fn set_scissor(&mut self, pass: &mut wgpu::RenderPass<'_>, scissor: ScissorRect) {
        let rect = (scissor.x0, scissor.y0, scissor.width(), scissor.height());
        if self.scissor != Some(rect) {
            self.scissor_changes += 1;
            pass.insert_debug_marker("pebbles.gpu.clip.scissor");
            pass.set_scissor_rect(rect.0, rect.1, rect.2, rect.3);
            self.scissor = Some(rect);
        }
    }
}

/// Resolve glyphs and images for the whole frame, then upload each arena once (10 §2).
pub(crate) fn resolve_and_upload(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    stats: &mut FrameStats,
) {
    resolve_glyphs(&mut renderer.glyph_atlas, &renderer.scratch.plan, &mut renderer.glyph_instances, stats);
    super::resolve::cull_resolved(&renderer.scratch.plan, &mut renderer.glyph_instances);
    // SYNC (10 §5.7): grow (separate submit) before this frame's uploads, then refresh the bind group.
    renderer.glyph_atlas.flush(device, queue, &renderer.layouts.glyph, stats);
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
        renderer.image_instances.push(if drawable { req.quad } else { bytemuck::Zeroable::zeroed() });
    }
    let arenas = &mut renderer.arenas;
    arenas.glyphs.upload(device, queue, bytemuck::cast_slice(&renderer.glyph_instances), stats);
    arenas.images.upload(device, queue, bytemuck::cast_slice(&renderer.image_instances), stats);
}

/// Draw every command of the renderer's current plan, in order, into `pass`.
/// The pipeline family a draw command uses.
pub(crate) fn cmd_kind(cmd: &DrawCmd) -> PipelineKind {
    match cmd {
        DrawCmd::Solid { .. } => PipelineKind::Solid,
        DrawCmd::Shapes { .. } => PipelineKind::Sdf,
        DrawCmd::Shadows { .. } => PipelineKind::Shadow,
        DrawCmd::Glyphs { .. } => PipelineKind::Glyph,
        DrawCmd::Images { .. } => PipelineKind::Image,
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
    for kind in kinds {
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

pub(crate) fn encode(
    renderer: &Renderer,
    pass: &mut wgpu::RenderPass<'_>,
    format: wgpu::TextureFormat,
    stats: &mut FrameStats,
    composites: &[super::tier_c::Composite],
) {
    pass.set_bind_group(0, renderer.globals_bg(), &[0]);
    stats.bind_group_switches += 1;
    // The main pass is single-sampled (R17).
    let sample_count = 1;
    let key = |kind, variant| PipelineKey { kind, color_format: format, sample_count, blend: 0, variant };
    let mut state = PassState::new();

    let mut runs = composites.iter().peekable();
    // Debug groups (S52): one per maximal run of same-kind draws; balanced within the pass.
    let groups = debug_group_runs(&renderer.scratch.plan.cmds);
    let (mut g, mut open) = (0, false);
    let mut i = 0;
    while let Some(cmd) = renderer.scratch.plan.cmds.get(i) {
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
            state.set_scissor(pass, c.run.scissor);
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
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                state.set_scissor(pass, clip.scissor);
                pass.draw_indexed(indices.clone(), 0, 0..1);
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
                pass.set_vertex_buffer(0, vb.slice(..));
                state.set_scissor(pass, clip.scissor);
                pass.draw(0..4, instances.clone());
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
                pass.set_vertex_buffer(0, vb.slice(..));
                state.set_scissor(pass, clip.scissor);
                pass.draw(0..4, instances.clone());
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
                state.set_scissor(pass, clip.scissor);
                state.set_pipeline(pass, pipeline);
                pass.set_bind_group(1, bg, &[]);
                stats.bind_group_switches += 1;
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.draw(0..4, glyphs.clone());
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
                state.set_scissor(pass, clip.scissor);
                state.set_pipeline(pass, pipeline);
                pass.set_bind_group(1, bg, &[]);
                stats.bind_group_switches += 1;
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.draw(0..4, images.clone());
                stats.draw_calls += 1;
            }
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
    state.set_scissor(pass, ScissorRect::full(renderer.scratch.plan.target_size));
}

#[cfg(test)]
mod tests {
    use super::{cmd_kind, debug_group_runs};
    use crate::prepare::{DrawCmd, ScissorRect};

    #[test]
    fn debug_group_runs_partition_commands() {
        let clip = crate::prepare::ClipState { scissor: ScissorRect::full((8, 8)) };
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
