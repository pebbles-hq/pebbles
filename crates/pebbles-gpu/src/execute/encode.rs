//! The single ordered encode pass: iterate `plan.cmds` and draw each command in painter's order.

use pebbles_render::paint::Scene;

use super::super::prepare::{DrawCmd, ImageQuad, ScissorRect, image_uploads};
use super::super::renderer::Renderer;
use super::pipelines::{PipelineKey, PipelineKind};
use super::resolve::{GlyphGeometry, build_glyphs};
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

/// Draw every command of the renderer's current plan, in order, into `pass`.
pub(crate) fn encode(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pass: &mut wgpu::RenderPass<'_>,
    scene: &Scene,
    format: wgpu::TextureFormat,
    stats: &mut FrameStats,
) {
    pass.set_bind_group(0, renderer.globals_bg(), &[0]);

    let cmds = renderer.scratch.plan.cmds.clone();
    let uploads = image_uploads(scene);
    let sample_count = renderer.sample_count;
    let mut state = PassState::new();

    for cmd in &cmds {
        match cmd {
            DrawCmd::Solid { indices, clip } => {
                let key = PipelineKey {
                    kind: PipelineKind::Solid,
                    color_format: format,
                    sample_count,
                    blend: 0,
                    variant: 0,
                };
                let Some(pipeline) = renderer.pipelines.get(key).cloned() else { continue };
                pass.set_pipeline(&pipeline);
                if let Some(vb) = renderer.arenas.solid_vertices.buffer() {
                    pass.set_vertex_buffer(0, vb.slice(..));
                }
                if let Some(ib) = renderer.arenas.solid_indices.buffer() {
                    pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                }
                state.set_scissor(pass, clip.scissor);
                pass.draw_indexed(indices.clone(), 0, 0..1);
                stats.draw_calls += 1;
            }
            DrawCmd::Shadows { instances, clip } => {
                let key = PipelineKey {
                    kind: PipelineKind::Shadow,
                    color_format: format,
                    sample_count,
                    blend: 0,
                    variant: 0,
                };
                let Some(pipeline) = renderer.pipelines.get(key).cloned() else { continue };
                pass.set_pipeline(&pipeline);
                if let Some(vb) = renderer.arenas.shadow_instances.buffer() {
                    pass.set_vertex_buffer(0, vb.slice(..));
                }
                state.set_scissor(pass, clip.scissor);
                pass.draw(0..4, instances.clone());
                stats.draw_calls += 1;
            }
            DrawCmd::Glyphs { glyphs, clip } => {
                let runs =
                    renderer.scratch.plan.glyph_runs[glyphs.start as usize..glyphs.end as usize].to_vec();
                state.set_scissor(pass, clip.scissor);
                draw_glyphs(renderer, device, queue, pass, &runs, format, stats);
            }
            DrawCmd::Images { images, clip } => {
                let imgs = renderer.scratch.plan.images[images.start as usize..images.end as usize].to_vec();
                state.set_scissor(pass, clip.scissor);
                draw_images(renderer, device, queue, pass, &imgs, &uploads, format, stats);
            }
        }
    }
}

fn glyph_key(format: wgpu::TextureFormat, sample_count: u32, variant: u8) -> PipelineKey {
    PipelineKey { kind: PipelineKind::Glyph, color_format: format, sample_count, blend: 0, variant }
}

fn glyph_bind_group(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    color: bool,
    stats: &mut FrameStats,
) -> Option<wgpu::BindGroup> {
    if let Some(bg) = renderer.glyph_bg(color) {
        return Some(bg.clone());
    }
    let atlas = if color { renderer.color_atlas.as_ref()? } else { renderer.atlas.as_ref()? };
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &renderer.layouts.texture,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&atlas.view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&atlas.sampler) },
        ],
    });
    stats.bind_groups_created += 1;
    renderer.set_glyph_bg(color, bg.clone());
    Some(bg)
}

fn draw_glyphs(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pass: &mut wgpu::RenderPass<'_>,
    runs: &[super::super::prepare::GlyphRun],
    format: wgpu::TextureFormat,
    stats: &mut FrameStats,
) {
    let (coverage, color) = build_glyphs(renderer, device, queue, runs, stats);
    let sample_count = renderer.sample_count;
    if !coverage.is_empty() {
        if let Some(bg) = glyph_bind_group(renderer, device, false, stats)
            && let Some(pipeline) = renderer.pipelines.get(glyph_key(format, sample_count, 0)).cloned()
        {
            draw_glyph_geometry(renderer, device, queue, pass, &coverage, &pipeline, &bg, stats);
        }
    }
    if !color.is_empty() {
        if let Some(bg) = glyph_bind_group(renderer, device, true, stats)
            && let Some(pipeline) = renderer.pipelines.get(glyph_key(format, sample_count, 1)).cloned()
        {
            draw_glyph_geometry(renderer, device, queue, pass, &color, &pipeline, &bg, stats);
        }
    }
}

fn draw_glyph_geometry(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pass: &mut wgpu::RenderPass<'_>,
    geom: &GlyphGeometry,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    stats: &mut FrameStats,
) {
    renderer.arenas.glyph_vertices.upload(device, queue, bytemuck::cast_slice(&geom.vertices), stats);
    renderer.arenas.glyph_indices.upload(device, queue, bytemuck::cast_slice(&geom.indices), stats);
    let Some(vb) = renderer.arenas.glyph_vertices.buffer() else { return };
    let Some(ib) = renderer.arenas.glyph_indices.buffer() else { return };
    pass.set_pipeline(pipeline);
    pass.set_bind_group(1, bind_group, &[]);
    pass.set_vertex_buffer(0, vb.slice(..));
    pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
    pass.draw_indexed(0..super::super::prepare::num::index_u32(geom.indices.len()), 0, 0..1);
    stats.draw_calls += 1;
}

fn image_key(format: wgpu::TextureFormat, sample_count: u32) -> PipelineKey {
    PipelineKey { kind: PipelineKind::Image, color_format: format, sample_count, blend: 0, variant: 0 }
}

fn draw_images(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pass: &mut wgpu::RenderPass<'_>,
    images: &[ImageQuad],
    uploads: &[super::super::prepare::ImageUpload],
    format: wgpu::TextureFormat,
    stats: &mut FrameStats,
) {
    if images.is_empty() {
        return;
    }
    let Some(pipeline) = renderer.pipelines.get(image_key(format, renderer.sample_count)).cloned() else {
        return;
    };
    // Flatten all quads into one vertex array for the arena.
    let mut vertices: Vec<crate::gpu_types::ImageVertex> = Vec::with_capacity(images.len() * 4);
    for quad in images {
        vertices.extend_from_slice(&quad.vertices);
    }
    renderer.arenas.image_vertices.upload(device, queue, bytemuck::cast_slice(&vertices), stats);
    let Some(vb) = renderer.arenas.image_vertices.buffer() else { return };

    for (i, quad) in images.iter().enumerate() {
        let Some(upload) = uploads.get(quad.image_index as usize) else { continue };
        let entry = renderer.image_cache.entry(upload.id).or_insert_with(|| {
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
                label: None,
                layout: &renderer.layouts.texture,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&renderer.image_sampler),
                    },
                ],
            });
            stats.bind_groups_created += 1;
            (texture, view, bind_group)
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(1, &entry.2, &[]);
        pass.set_vertex_buffer(0, vb.slice(..));
        // The static index buffer [0,1,2,0,2,3] with base_vertex = 4*i selects this quad.
        pass.set_index_buffer(renderer.image_index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..6, (i as i32) * 4, 0..1);
        stats.draw_calls += 1;
    }
}
