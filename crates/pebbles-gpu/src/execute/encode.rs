//! The single ordered encode pass: iterate `plan.cmds` and draw each command in painter's order.

use pebbles_render::paint::Scene;

use super::super::prepare::num::index_u32;
use super::super::prepare::{DrawCmd, ImageUpload, ScissorRect, image_uploads};
use super::super::renderer::Renderer;
use super::pipelines::{PipelineKey, PipelineKind};
use super::resolve::resolve_glyphs;
use crate::stats::FrameStats;

/// Tracks the render pass's current scissor so redundant `set_scissor_rect` calls are elided.
struct PassState {
    scissor: Option<(u32, u32, u32, u32)>,
}

impl PassState {
    fn new() -> Self {
        Self { scissor: None }
    }

    fn set_scissor(&mut self, pass: &mut wgpu::RenderPass<'_>, scissor: ScissorRect) {
        let rect = (scissor.x0, scissor.y0, scissor.width(), scissor.height());
        if self.scissor != Some(rect) {
            pass.set_scissor_rect(rect.0, rect.1, rect.2, rect.3);
            self.scissor = Some(rect);
        }
    }
}

/// Per-command image data resolved before the pass.
#[derive(Clone, Default)]
enum Resolved {
    #[default]
    None,
    Images {
        first_quad: u32,
        textures: Vec<u64>,
    },
}

/// Everything resolved before the pass: glyph quads and image quads for the whole frame,
/// so each arena is written exactly once per submit (10 §2).
#[derive(Default)]
pub(crate) struct ResolvedFrame {
    per_cmd: Vec<Resolved>,
    image_vertices: Vec<crate::gpu_types::ImageVertex>,
}

/// Resolve glyphs and images for every command, then upload each arena once.
pub(crate) fn resolve_and_upload(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    scene: &Scene,
    stats: &mut FrameStats,
) -> ResolvedFrame {
    let mut frame = ResolvedFrame::default();
    let uploads = image_uploads(scene);
    let cmds = std::mem::take(&mut renderer.scratch.plan.cmds);
    for cmd in &cmds {
        let resolved = match cmd {
            DrawCmd::Images { images, .. } => {
                let first_quad = index_u32(frame.image_vertices.len() / 4);
                let mut textures = Vec::new();
                let quads = renderer.scratch.plan.images.get(images.start as usize..images.end as usize);
                for quad in quads.unwrap_or_default().to_vec() {
                    let Some(upload) = uploads.get(quad.image_index as usize) else { continue };
                    ensure_image(renderer, device, queue, upload, stats);
                    frame.image_vertices.extend_from_slice(&quad.vertices);
                    textures.push(upload.id);
                }
                Resolved::Images { first_quad, textures }
            }
            _ => Resolved::None,
        };
        frame.per_cmd.push(resolved);
    }
    renderer.scratch.plan.cmds = cmds;
    resolve_glyphs(
        &mut renderer.glyph_atlas,
        queue,
        &renderer.scratch.plan,
        &mut renderer.glyph_instances,
        stats,
    );
    let arenas = &mut renderer.arenas;
    arenas.glyphs.upload(device, queue, bytemuck::cast_slice(&renderer.glyph_instances), stats);
    arenas.image_vertices.upload(device, queue, bytemuck::cast_slice(&frame.image_vertices), stats);
    frame
}

/// Draw every command of the renderer's current plan, in order, into `pass`.
pub(crate) fn encode(
    renderer: &Renderer,
    frame: &ResolvedFrame,
    pass: &mut wgpu::RenderPass<'_>,
    format: wgpu::TextureFormat,
    stats: &mut FrameStats,
) {
    pass.set_bind_group(0, renderer.globals_bg(), &[0]);
    let sample_count = renderer.sample_count;
    let key = |kind, variant| PipelineKey { kind, color_format: format, sample_count, blend: 0, variant };
    let mut state = PassState::new();

    for (cmd, resolved) in renderer.scratch.plan.cmds.iter().zip(&frame.per_cmd) {
        match (cmd, resolved) {
            (DrawCmd::Solid { indices, clip }, _) => {
                let (Some(pipeline), Some(vb), Some(ib)) = (
                    renderer.pipelines.get(key(PipelineKind::Solid, 0)),
                    renderer.arenas.solid_vertices.buffer(),
                    renderer.arenas.solid_indices.buffer(),
                ) else {
                    continue;
                };
                pass.set_pipeline(pipeline);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                state.set_scissor(pass, clip.scissor);
                pass.draw_indexed(indices.clone(), 0, 0..1);
                stats.draw_calls += 1;
            }
            (DrawCmd::Shadows { instances, clip }, _) => {
                let (Some(pipeline), Some(vb)) = (
                    renderer.pipelines.get(key(PipelineKind::Shadow, 0)),
                    renderer.arenas.shadow_instances.buffer(),
                ) else {
                    continue;
                };
                pass.set_pipeline(pipeline);
                pass.set_vertex_buffer(0, vb.slice(..));
                state.set_scissor(pass, clip.scissor);
                pass.draw(0..4, instances.clone());
                stats.draw_calls += 1;
            }
            (DrawCmd::Glyphs { glyphs, clip }, _) => {
                let (Some(pipeline), Some(vb)) =
                    (renderer.pipelines.get(key(PipelineKind::Glyph, 0)), renderer.arenas.glyphs.buffer())
                else {
                    continue;
                };
                state.set_scissor(pass, clip.scissor);
                pass.set_pipeline(pipeline);
                pass.set_bind_group(1, &renderer.glyph_atlas.bind_group, &[]);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.draw(0..4, glyphs.clone());
                stats.draw_calls += 1;
            }
            (DrawCmd::Images { clip, .. }, Resolved::Images { first_quad, textures }) => {
                let (Some(pipeline), Some(vb)) = (
                    renderer.pipelines.get(key(PipelineKind::Image, 0)),
                    renderer.arenas.image_vertices.buffer(),
                ) else {
                    continue;
                };
                state.set_scissor(pass, clip.scissor);
                pass.set_pipeline(pipeline);
                pass.set_vertex_buffer(0, vb.slice(..));
                // The static index buffer [0,1,2,0,2,3] with base_vertex = 4*quad selects each quad.
                pass.set_index_buffer(renderer.image_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
                for (i, id) in textures.iter().enumerate() {
                    let Some(entry) = renderer.image_cache.get(id) else { continue };
                    let Ok(base) = i32::try_from((first_quad + index_u32(i)) * 4) else { continue };
                    pass.set_bind_group(1, &entry.2, &[]);
                    pass.draw_indexed(0..6, base, 0..1);
                    stats.draw_calls += 1;
                }
            }
            _ => {}
        }
    }
}

/// Upload an image once and cache its texture, view and bind group by blob id.
fn ensure_image(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    upload: &ImageUpload,
    stats: &mut FrameStats,
) {
    if renderer.image_cache.contains_key(&upload.id) {
        return;
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.image"),
        size: wgpu::Extent3d {
            width: upload.width.max(1),
            height: upload.height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    stats.textures_created += 1;
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &upload.rgba8,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(upload.width * 4),
            rows_per_image: Some(upload.height),
        },
        wgpu::Extent3d { width: upload.width, height: upload.height, depth_or_array_layers: 1 },
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    stats.texture_views_created += 1;
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("pebbles.gpu.bind.image"),
        layout: &renderer.layouts.texture,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&renderer.image_sampler),
            },
        ],
    });
    stats.bind_groups_created += 1;
    renderer.image_cache.insert(upload.id, (texture, view, bind_group));
}
