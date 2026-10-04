//! One [`Renderer`] per device: pipelines, caches, atlases; `render`.

use std::collections::HashMap;

use cosmic_text::{FontSystem, SwashCache};
use wgpu::util::DeviceExt;

use pebbles_render::paint::Scene;

use super::context::{DeviceId, GpuDevice};
use super::error::GpuError;
use super::execute::pipelines::{GlyphPipeline, ImagePipeline, ShadowPipeline, make_pipeline};
use super::execute::resources::Atlas;
use super::execute::{draw_glyphs, draw_images, draw_shadows, scissor_params};
use super::frame_targets::create_msaa;
use super::prepare::mesh;

/// Antialiasing selector — `Area` maps to 4× MSAA.
#[derive(Clone, Copy, Debug)]
pub enum AaConfig {
    /// The Phase-1 area-antialiasing path.
    Area,
}

/// Multisample count for the given antialiasing config.
const fn aa_sample_count(aa: AaConfig) -> u32 {
    match aa {
        AaConfig::Area => 4,
    }
}

/// Per-frame render parameters.
pub struct RenderParams {
    /// The clear (base) color.
    pub base_color: peniko::Color,
    /// Render size in physical pixels.
    pub width: u32,
    /// Render size in physical pixels.
    pub height: u32,
    /// The antialiasing method.
    pub antialiasing_method: AaConfig,
}

/// Pipelines + caches for ONE device. Shared by every surface/target on that device.
pub struct Renderer {
    pipeline: HashMap<(wgpu::TextureFormat, u32), (wgpu::RenderPipeline, wgpu::Buffer, wgpu::BindGroup)>,
    msaa: Option<(wgpu::TextureFormat, u32, u32, wgpu::Texture)>,
    pub(crate) sample_count: u32,
    pub(crate) fonts: Option<FontSystem>,
    pub(crate) swash: Option<SwashCache>,
    pub(crate) font_ids: HashMap<(u64, u32), fontdb::ID>,
    pub(crate) atlas: Option<Atlas>,
    pub(crate) color_atlas: Option<Atlas>,
    pub(crate) glyph_pipeline: HashMap<(wgpu::TextureFormat, u32), GlyphPipeline>,
    pub(crate) color_pipeline: HashMap<(wgpu::TextureFormat, u32), GlyphPipeline>,
    pub(crate) image_cache: HashMap<u64, wgpu::Texture>,
    pub(crate) image_pipeline: HashMap<(wgpu::TextureFormat, u32), ImagePipeline>,
    pub(crate) shadow_pipeline: HashMap<(wgpu::TextureFormat, u32), ShadowPipeline>,
}

impl Renderer {
    /// Create an empty renderer for a device (the device is not yet used; resources are
    /// built lazily on the first frame).
    pub fn new(_gpu: &GpuDevice, _id: DeviceId) -> Self {
        Renderer {
            pipeline: HashMap::new(),
            msaa: None,
            sample_count: aa_sample_count(AaConfig::Area),
            fonts: None,
            swash: None,
            font_ids: HashMap::new(),
            atlas: None,
            color_atlas: None,
            glyph_pipeline: HashMap::new(),
            color_pipeline: HashMap::new(),
            image_cache: HashMap::new(),
            image_pipeline: HashMap::new(),
            shadow_pipeline: HashMap::new(),
        }
    }

    /// The windowed frame loop's entry point: render `scene` into `target` of `format`.
    pub fn render_to_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        target: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        params: &RenderParams,
    ) -> Result<(), GpuError> {
        self.sample_count = aa_sample_count(params.antialiasing_method);
        self.render_offscreen(
            device,
            queue,
            scene,
            target,
            params.base_color,
            (params.width, params.height),
            format,
        )
    }

    /// Render `scene` into `target` of a known `format` (the shared, target-agnostic entry).
    #[allow(clippy::too_many_arguments)]
    pub fn render_offscreen(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        target: &wgpu::TextureView,
        base_color: peniko::Color,
        size: (u32, u32),
        format: wgpu::TextureFormat,
    ) -> Result<(), GpuError> {
        let size = (size.0.max(1), size.1.max(1));
        let key = (format, self.sample_count);
        if !self.pipeline.contains_key(&key) {
            let (_, p, u, bg) = make_pipeline(device, format, self.sample_count);
            self.pipeline.insert(key, (p, u, bg));
        }
        let (pipeline, uniform, bind_group) = self.pipeline.get(&key).expect("pipeline just initialized");
        queue.write_buffer(uniform, 0, bytemuck::cast_slice(&[size.0 as f32, size.1 as f32, 0.0, 0.0]));
        if self.msaa.as_ref().is_none_or(|m| m.0 != format || m.1 != size.0 || m.2 != size.1) {
            let texture = create_msaa(device, format, size.0, size.1, self.sample_count);
            self.msaa = Some((format, size.0, size.1, texture));
        }
        let msaa = &self.msaa.as_ref().expect("msaa just initialized").3;

        let mesh = mesh(scene);
        let [r, g, b, a] = base_color.components;
        let mut encoder = device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("pebbles.gpu.encoder") });
        {
            let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pebbles.gpu.pass.root"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &msaa_view,
                    depth_slice: None,
                    resolve_target: Some(target),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(r),
                            g: f64::from(g),
                            b: f64::from(b),
                            a: f64::from(a),
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !mesh.solid.is_empty() {
                let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("pebbles.gpu.buffer.vertices"),
                    contents: bytemuck::cast_slice(&mesh.solid.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("pebbles.gpu.buffer.indices"),
                    contents: bytemuck::cast_slice(&mesh.solid.indices),
                    usage: wgpu::BufferUsages::INDEX,
                });
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, bind_group, &[]);
                pass.set_vertex_buffer(0, vertices.slice(..));
                pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
                for batch in &mesh.solid.batches {
                    let (x, y, w, h) = scissor_params(batch.scissor, size.0, size.1);
                    pass.set_scissor_rect(x, y, w, h);
                    pass.draw_indexed(batch.range.clone(), 0, 0..1);
                }
            }
            if !mesh.glyphs.is_empty() {
                draw_glyphs(self, device, queue, &mut pass, &mesh.glyphs, format, size.0, size.1);
            }
            if !mesh.shadows.is_empty() {
                draw_shadows(self, device, queue, &mut pass, &mesh.shadows, format, size.0, size.1);
            }
            if !mesh.images.is_empty() {
                draw_images(self, device, queue, &mut pass, &mesh.images, scene, format, size.0, size.1);
            }
        }
        queue.submit([encoder.finish()]);
        Ok(())
    }
}
