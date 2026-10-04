//! The ordered draw/encode helpers: glyph quads, scissor clamps and the draw functions.

use pebbles_render::paint::Scene;
use wgpu::util::DeviceExt;

use super::super::prepare::{GlyphRun, ImageQuad, Scissor, image_uploads};
use super::super::renderer::Renderer;
use super::pipelines::{GlyphPipeline, make_glyph_pipeline, make_image_pipeline, make_shadow_pipeline};
use super::resolve::build_glyphs;
use super::resources::ATLAS_SIZE;
use crate::gpu_types::{GlyphVertex, ShadowVertex};

/// An index range into the glyph vertex/index buffers plus the scissor rect active
/// while drawing it.
struct GlyphBatch {
    range: std::ops::Range<u32>,
    scissor: Option<Scissor>,
}

/// Vertex/index geometry for one glyph pipeline (coverage or color), batched by
/// scissor rect.
pub(crate) struct GlyphDraw {
    vertices: Vec<GlyphVertex>,
    indices: Vec<u32>,
    batches: Vec<GlyphBatch>,
}

impl GlyphDraw {
    pub(crate) fn new() -> Self {
        Self { vertices: Vec::new(), indices: Vec::new(), batches: Vec::new() }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub(crate) fn push_quad(&mut self, quad: [GlyphVertex; 4], scissor: Option<Scissor>) {
        if self.batches.last().is_none_or(|b| b.scissor != scissor) {
            let start = self.indices.len() as u32;
            self.batches.push(GlyphBatch { range: start..start, scissor });
        }
        let base = self.vertices.len() as u32;
        self.vertices.extend_from_slice(&quad);
        self.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        self.batches.last_mut().expect("batch just pushed").range.end = self.indices.len() as u32;
    }
}

/// Clamp a scissor rect (physical-pixel space) to the render-target bounds.
pub(crate) fn scissor_params(scissor: Option<Scissor>, width: u32, height: u32) -> (u32, u32, u32, u32) {
    match scissor {
        None => (0, 0, width, height),
        Some(s) => {
            let x = s.x.min(width);
            let y = s.y.min(height);
            let w = s.width.min(width - x);
            let h = s.height.min(height - y);
            (x, y, w, h)
        }
    }
}

/// Build the four vertices of a glyph quad: physical-pixel position + atlas UVs.
#[allow(clippy::too_many_arguments)]
pub(crate) fn glyph_quad(
    region: super::resources::AtlasRegion,
    w: u32,
    h: u32,
    x: i32,
    y: i32,
    placement_left: i32,
    placement_top: i32,
    tint: [f32; 4],
) -> [GlyphVertex; 4] {
    let s = ATLAS_SIZE as f32;
    let u0 = region.x as f32 / s;
    let v0 = region.y as f32 / s;
    let u1 = (region.x + w) as f32 / s;
    let v1 = (region.y + h) as f32 / s;
    let left = x as f32 + placement_left as f32;
    let top = y as f32 - placement_top as f32;
    let right = left + w as f32;
    let bottom = top + h as f32;
    [
        GlyphVertex { position: [left, top], uv: [u0, v0], color: tint },
        GlyphVertex { position: [right, top], uv: [u1, v0], color: tint },
        GlyphVertex { position: [right, bottom], uv: [u1, v1], color: tint },
        GlyphVertex { position: [left, bottom], uv: [u0, v1], color: tint },
    ]
}

/// Draw one glyph set (coverage or color) with its pipeline.
pub(crate) fn draw_glyph_set(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pass: &mut wgpu::RenderPass<'_>,
    draw: &GlyphDraw,
    gp: &GlyphPipeline,
    width: u32,
    height: u32,
) {
    queue.write_buffer(&gp.uniform, 0, bytemuck::cast_slice(&[width as f32, height as f32, 0.0, 0.0]));
    let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("pebbles.gpu.glyph.vertices"),
        contents: bytemuck::cast_slice(&draw.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("pebbles.gpu.glyph.indices"),
        contents: bytemuck::cast_slice(&draw.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    pass.set_pipeline(&gp.pipeline);
    pass.set_bind_group(0, &gp.bind_group, &[]);
    pass.set_vertex_buffer(0, vbuf.slice(..));
    pass.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
    for batch in &draw.batches {
        let (x, y, w, h) = scissor_params(batch.scissor, width, height);
        pass.set_scissor_rect(x, y, w, h);
        pass.draw_indexed(batch.range.clone(), 0, 0..1);
    }
}

/// Rasterize the frame's glyphs and draw them as tinted textured quads.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_glyphs(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pass: &mut wgpu::RenderPass<'_>,
    runs: &[GlyphRun],
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) {
    let (coverage, color) = build_glyphs(renderer, device, queue, runs);
    if !coverage.is_empty() {
        let atlas = renderer.atlas.as_ref().expect("atlas built by build_glyphs");
        if renderer.glyph_pipeline.as_ref().is_none_or(|p| p.0 != format) {
            let gp = make_glyph_pipeline(device, format, atlas, renderer.sample_count, false);
            renderer.glyph_pipeline = Some((format, gp));
        }
        let (_, gp) = renderer.glyph_pipeline.as_ref().expect("glyph pipeline just built");
        draw_glyph_set(device, queue, pass, &coverage, gp, width, height);
    }
    if !color.is_empty() {
        let atlas = renderer.color_atlas.as_ref().expect("color atlas built by build_glyphs");
        if renderer.color_pipeline.as_ref().is_none_or(|p| p.0 != format) {
            let gp = make_glyph_pipeline(device, format, atlas, renderer.sample_count, true);
            renderer.color_pipeline = Some((format, gp));
        }
        let (_, gp) = renderer.color_pipeline.as_ref().expect("color pipeline just built");
        draw_glyph_set(device, queue, pass, &color, gp, width, height);
    }
}

/// Draw the analytic rounded-rect shadows (one draw call).
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_shadows(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pass: &mut wgpu::RenderPass<'_>,
    shadows: &[ShadowVertex],
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) {
    if renderer.shadow_pipeline.as_ref().is_none_or(|p| p.0 != format) {
        renderer.shadow_pipeline =
            Some((format, make_shadow_pipeline(device, format, renderer.sample_count)));
    }
    let (_, sp) = renderer.shadow_pipeline.as_ref().expect("shadow pipeline just built");
    queue.write_buffer(&sp.uniform, 0, bytemuck::cast_slice(&[width as f32, height as f32, 0.0, 0.0]));
    let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("pebbles.gpu.shadow.vertices"),
        contents: bytemuck::cast_slice(shadows),
        usage: wgpu::BufferUsages::VERTEX,
    });
    pass.set_pipeline(&sp.pipeline);
    pass.set_bind_group(0, &sp.bind_group, &[]);
    pass.set_vertex_buffer(0, vbuf.slice(..));
    pass.draw(0..shadows.len() as u32, 0..1);
}

/// Upload image pixels (cached by id) and draw each image quad with its texture.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_images(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pass: &mut wgpu::RenderPass<'_>,
    images: &[ImageQuad],
    scene: &Scene,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) {
    let uploads = image_uploads(scene);
    if uploads.is_empty() {
        return;
    }
    if renderer.image_pipeline.as_ref().is_none_or(|p| p.0 != format) {
        renderer.image_pipeline = Some((format, make_image_pipeline(device, format, renderer.sample_count)));
    }
    let (_, ip) = renderer.image_pipeline.as_ref().expect("image pipeline just built");
    queue.write_buffer(&ip.uniform, 0, bytemuck::cast_slice(&[width as f32, height as f32, 0.0, 0.0]));
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
        let (x, y, w, h) = scissor_params(quad.scissor, width, height);
        pass.set_scissor_rect(x, y, w, h);
        pass.set_pipeline(&ip.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.set_vertex_buffer(0, vbuf.slice(..));
        pass.set_index_buffer(ip.index_buffer.slice(..), wgpu::IndexFormat::Uint32);
        pass.draw_indexed(0..6, 0, 0..1);
    }
}
