//! The single ordered encode pass: iterate `plan.cmds` and draw each command in painter's order.

use pebbles_render::paint::Scene;
use wgpu::util::DeviceExt;

use super::super::prepare::{DrawCmd, ImageQuad, ScissorRect, image_uploads};
use super::super::renderer::Renderer;
use super::pipelines::{
    GlyphPipeline, make_glyph_pipeline, make_image_pipeline, make_pipeline, make_shadow_pipeline,
};
use super::resolve::{GlyphGeometry, build_glyphs};

/// Clamp a scissor rect (physical-pixel corners) to the render-target bounds.
pub(crate) fn scissor_params(scissor: ScissorRect, width: u32, height: u32) -> (u32, u32, u32, u32) {
    let x0 = scissor.x0.min(width);
    let y0 = scissor.y0.min(height);
    let x1 = scissor.x1.min(width);
    let y1 = scissor.y1.min(height);
    (x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
}

/// Draw every command of the renderer's current plan, in order, into `pass`.
pub(crate) fn encode(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pass: &mut wgpu::RenderPass<'_>,
    scene: &Scene,
    format: wgpu::TextureFormat,
    size: (u32, u32),
) {
    // Upload the shared geometry once (borrows end here).
    let solid_vertices = (!renderer.scratch.plan.solid_vertices.is_empty()).then(|| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pebbles.gpu.buffer.vertices"),
            contents: bytemuck::cast_slice(&renderer.scratch.plan.solid_vertices),
            usage: wgpu::BufferUsages::VERTEX,
        })
    });
    let solid_indices = (!renderer.scratch.plan.solid_indices.is_empty()).then(|| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pebbles.gpu.buffer.indices"),
            contents: bytemuck::cast_slice(&renderer.scratch.plan.solid_indices),
            usage: wgpu::BufferUsages::INDEX,
        })
    });
    let shadow_instances = (!renderer.scratch.plan.shadows.is_empty()).then(|| {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pebbles.gpu.buffer.shadows"),
            contents: bytemuck::cast_slice(&renderer.scratch.plan.shadows),
            usage: wgpu::BufferUsages::VERTEX,
        })
    });
    let cmds = renderer.scratch.plan.cmds.clone();
    let uploads = image_uploads(scene);

    if std::env::var("PEBBLES_DEBUG_PLAN").is_ok() {
        eprintln!(
            "encode: cmds={} solid_v={} shadows={} glyph_runs={} images={}",
            cmds.len(),
            renderer.scratch.plan.solid_vertices.len(),
            renderer.scratch.plan.shadows.len(),
            renderer.scratch.plan.glyph_runs.len(),
            renderer.scratch.plan.images.len(),
        );
        for (i, cmd) in cmds.iter().enumerate() {
            eprintln!("  cmd[{i}] = {cmd:?}");
        }
    }

    for cmd in &cmds {
        match cmd {
            DrawCmd::Solid { indices, clip } => {
                let (pipeline, uniform, bind_group) = solid_pipeline(renderer, device, format);
                queue.write_buffer(
                    uniform,
                    0,
                    bytemuck::cast_slice(&[size.0 as f32, size.1 as f32, 0.0, 0.0]),
                );
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, bind_group, &[]);
                pass.set_vertex_buffer(0, solid_vertices.as_ref().expect("solid vertices").slice(..));
                pass.set_index_buffer(
                    solid_indices.as_ref().expect("solid indices").slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                set_scissor(pass, clip.scissor, size);
                pass.draw_indexed(indices.clone(), 0, 0..1);
            }
            DrawCmd::Shadows { instances, clip } => {
                let sp = shadow_pipeline(renderer, device, format);
                queue.write_buffer(
                    &sp.uniform,
                    0,
                    bytemuck::cast_slice(&[size.0 as f32, size.1 as f32, 0.0, 0.0]),
                );
                pass.set_pipeline(&sp.pipeline);
                pass.set_bind_group(0, &sp.bind_group, &[]);
                pass.set_vertex_buffer(0, shadow_instances.as_ref().expect("shadow instances").slice(..));
                set_scissor(pass, clip.scissor, size);
                pass.draw(0..4, instances.clone());
            }
            DrawCmd::Glyphs { glyphs, clip } => {
                let runs =
                    renderer.scratch.plan.glyph_runs[glyphs.start as usize..glyphs.end as usize].to_vec();
                draw_glyphs(renderer, device, queue, pass, &runs, clip.scissor, format, size);
            }
            DrawCmd::Images { images, clip } => {
                let imgs = renderer.scratch.plan.images[images.start as usize..images.end as usize].to_vec();
                draw_images(renderer, device, queue, pass, &imgs, &uploads, clip.scissor, format, size);
            }
        }
    }
}

fn set_scissor(pass: &mut wgpu::RenderPass<'_>, scissor: ScissorRect, size: (u32, u32)) {
    let (x, y, w, h) = scissor_params(scissor, size.0, size.1);
    pass.set_scissor_rect(x, y, w, h);
}

fn solid_pipeline<'a>(
    renderer: &'a mut Renderer,
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (&'a wgpu::RenderPipeline, &'a wgpu::Buffer, &'a wgpu::BindGroup) {
    let key = (format, renderer.sample_count);
    if !renderer.pipeline.contains_key(&key) {
        let (_, p, u, bg) = make_pipeline(device, format, renderer.sample_count);
        renderer.pipeline.insert(key, (p, u, bg));
    }
    let (p, u, bg) = renderer.pipeline.get(&key).expect("pipeline just built");
    (p, u, bg)
}

fn shadow_pipeline<'a>(
    renderer: &'a mut Renderer,
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> &'a super::pipelines::ShadowPipeline {
    let key = (format, renderer.sample_count);
    if !renderer.shadow_pipeline.contains_key(&key) {
        renderer.shadow_pipeline.insert(key, make_shadow_pipeline(device, format, renderer.sample_count));
    }
    renderer.shadow_pipeline.get(&key).expect("shadow pipeline just built")
}

#[allow(clippy::too_many_arguments)]
fn draw_glyphs(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pass: &mut wgpu::RenderPass<'_>,
    runs: &[super::super::prepare::GlyphRun],
    scissor: ScissorRect,
    format: wgpu::TextureFormat,
    size: (u32, u32),
) {
    let (coverage, color) = build_glyphs(renderer, device, queue, runs);
    let key = (format, renderer.sample_count);
    if !coverage.is_empty() {
        let atlas = renderer.atlas.as_ref().expect("atlas built by build_glyphs");
        if !renderer.glyph_pipeline.contains_key(&key) {
            let gp = make_glyph_pipeline(device, format, atlas, renderer.sample_count, false);
            renderer.glyph_pipeline.insert(key, gp);
        }
        let gp = renderer.glyph_pipeline.get(&key).expect("glyph pipeline just built");
        draw_glyph_geometry(device, queue, pass, &coverage, gp, scissor, size);
    }
    if !color.is_empty() {
        let atlas = renderer.color_atlas.as_ref().expect("color atlas built by build_glyphs");
        if !renderer.color_pipeline.contains_key(&key) {
            let gp = make_glyph_pipeline(device, format, atlas, renderer.sample_count, true);
            renderer.color_pipeline.insert(key, gp);
        }
        let gp = renderer.color_pipeline.get(&key).expect("color pipeline just built");
        draw_glyph_geometry(device, queue, pass, &color, gp, scissor, size);
    }
}

fn draw_glyph_geometry(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pass: &mut wgpu::RenderPass<'_>,
    geom: &GlyphGeometry,
    gp: &GlyphPipeline,
    scissor: ScissorRect,
    size: (u32, u32),
) {
    queue.write_buffer(&gp.uniform, 0, bytemuck::cast_slice(&[size.0 as f32, size.1 as f32, 0.0, 0.0]));
    let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("pebbles.gpu.glyph.vertices"),
        contents: bytemuck::cast_slice(&geom.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("pebbles.gpu.glyph.indices"),
        contents: bytemuck::cast_slice(&geom.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    pass.set_pipeline(&gp.pipeline);
    pass.set_bind_group(0, &gp.bind_group, &[]);
    pass.set_vertex_buffer(0, vbuf.slice(..));
    pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
    set_scissor(pass, scissor, size);
    pass.draw_indexed(0..geom.indices.len() as u32, 0, 0..1);
}

#[allow(clippy::too_many_arguments)]
fn draw_images(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pass: &mut wgpu::RenderPass<'_>,
    images: &[ImageQuad],
    uploads: &[super::super::prepare::ImageUpload],
    scissor: ScissorRect,
    format: wgpu::TextureFormat,
    size: (u32, u32),
) {
    let key = (format, renderer.sample_count);
    if !renderer.image_pipeline.contains_key(&key) {
        renderer.image_pipeline.insert(key, make_image_pipeline(device, format, renderer.sample_count));
    }
    let ip = renderer.image_pipeline.get(&key).expect("image pipeline just built");
    queue.write_buffer(&ip.uniform, 0, bytemuck::cast_slice(&[size.0 as f32, size.1 as f32, 0.0, 0.0]));
    for quad in images {
        let Some(upload) = uploads.get(quad.image_index as usize) else { continue };
        let cache = &mut renderer.image_cache;
        let texture = cache.entry(upload.id).or_insert_with(|| {
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
            texture
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &ip.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: ip.uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&ip.sampler) },
            ],
        });
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pebbles.gpu.image.vertices"),
            contents: bytemuck::cast_slice(&quad.vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        set_scissor(pass, scissor, size);
        pass.set_pipeline(&ip.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_vertex_buffer(0, vbuf.slice(..));
        pass.set_index_buffer(ip.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..6, 0, 0..1);
    }
}
