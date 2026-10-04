//! Native wgpu host used by the transition backend. It intentionally owns the
//! instance, adapter/device pool, and swapchain instead of borrowing Vello utility
//! types, so subsequent renderer phases have no Vello dependency at this seam.

use std::collections::HashMap;
use std::mem::size_of;
use std::sync::OnceLock;

use bytemuck::{Pod, Zeroable};
use cosmic_text::{CacheKey, FontSystem, SwashCache, SwashContent};
use pebbles_render::Scene;
use pebbles_render::paint::{GlyphRun, ImageQuad, ImageVertex, Scissor, ShadowVertex, Vertex};
use wgpu::util::DeviceExt;

/// The swapchain texture format chosen at surface creation. Captured once so the
/// lazily-built solid pipeline targets the right format (mirrors `hybrid_host`).
static SURFACE_FORMAT: OnceLock<wgpu::TextureFormat> = OnceLock::new();

/// The glyph-atlas size (square). Fixed for v1 — a full document's distinct glyphs
/// fit comfortably; growth/eviction is a follow-up.
const ATLAS_SIZE: u32 = 1024;

/// A textured quad for the glyph pipeline: physical-pixel position + atlas UV + a
/// premultiplied tint color.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct GlyphVertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

pub(crate) struct DeviceHandle {
    adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

pub(crate) struct RenderContext {
    instance: wgpu::Instance,
    pub devices: Vec<DeviceHandle>,
}

pub(crate) struct RenderSurface<'window> {
    pub surface: wgpu::Surface<'window>,
    pub config: wgpu::SurfaceConfiguration,
    pub dev_id: usize,
    // Captured at surface creation so the lazily-built MSAA/solid targets match the
    // swapchain format. The renderer resolves through the shared `SURFACE_FORMAT` below.
    #[allow(dead_code)]
    pub format: wgpu::TextureFormat,
}

impl RenderContext {
    pub(crate) fn new() -> Self {
        Self { instance: wgpu::Instance::default(), devices: Vec::new() }
    }

    pub(crate) async fn create_surface<'window>(
        &mut self,
        window: impl Into<wgpu::SurfaceTarget<'window>>,
        width: u32,
        height: u32,
        present_mode: wgpu::PresentMode,
    ) -> Result<RenderSurface<'window>, String> {
        let surface = self.instance.create_surface(window).map_err(|error| error.to_string())?;
        self.create_render_surface(surface, width, height, present_mode).await
    }

    async fn create_render_surface<'window>(
        &mut self,
        surface: wgpu::Surface<'window>,
        width: u32,
        height: u32,
        present_mode: wgpu::PresentMode,
    ) -> Result<RenderSurface<'window>, String> {
        let dev_id = self.device(Some(&surface)).await.ok_or("no compatible GPU device")?;
        let device = &self.devices[dev_id];
        let capabilities = surface.get_capabilities(&device.adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or("surface exposes no supported texture format")?;
        let alpha_mode = capabilities.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto);
        SURFACE_FORMAT.get_or_init(|| format);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(&device.device, &config);
        Ok(RenderSurface { surface, config, dev_id, format })
    }

    pub(crate) fn resize_surface(&self, surface: &mut RenderSurface<'_>, width: u32, height: u32) {
        surface.config.width = width.max(1);
        surface.config.height = height.max(1);
        surface.surface.configure(&self.devices[surface.dev_id].device, &surface.config);
    }

    pub(crate) async fn device(&mut self, compatible_surface: Option<&wgpu::Surface<'_>>) -> Option<usize> {
        if let Some(index) = compatible_surface.and_then(|surface| {
            self.devices.iter().position(|device| device.adapter.is_surface_supported(surface))
        }) {
            return Some(index);
        }
        if compatible_surface.is_none() && !self.devices.is_empty() {
            return Some(0);
        }

        let adapter = self
            .instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface,
                force_fallback_adapter: false,
            })
            .await
            .ok()?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("pebbles.wgpu.device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await
            .ok()?;
        self.devices.push(DeviceHandle { adapter, device, queue });
        Some(self.devices.len() - 1)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum AaConfig {
    Area,
}

/// Multisample count for the given antialiasing config. `Area` maps to 4× MSAA —
/// the Phase-1 area-antialiasing path.
const fn aa_sample_count(aa: AaConfig) -> u32 {
    match aa {
        AaConfig::Area => 4,
    }
}

pub(crate) struct RenderParams {
    pub base_color: peniko::Color,
    pub width: u32,
    pub height: u32,
    pub antialiasing_method: AaConfig,
}

pub(crate) struct Renderer {
    pipeline: Option<(wgpu::TextureFormat, wgpu::RenderPipeline, wgpu::Buffer, wgpu::BindGroup)>,
    msaa: Option<(wgpu::TextureFormat, u32, u32, wgpu::Texture)>,
    sample_count: u32,
    // Glyph atlas + rasterization (Phase 2).
    fonts: Option<FontSystem>,
    swash: Option<SwashCache>,
    font_ids: HashMap<(u64, u32), fontdb::ID>,
    atlas: Option<Atlas>,
    color_atlas: Option<Atlas>,
    glyph_pipeline: Option<(wgpu::TextureFormat, GlyphPipeline)>,
    color_pipeline: Option<(wgpu::TextureFormat, GlyphPipeline)>,
    image_cache: HashMap<u64, wgpu::Texture>,
    image_pipeline: Option<(wgpu::TextureFormat, ImagePipeline)>,
    shadow_pipeline: Option<(wgpu::TextureFormat, ShadowPipeline)>,
}

pub(crate) fn new_renderer(_device: &wgpu::Device, _queue: &wgpu::Queue) -> Renderer {
    Renderer {
        pipeline: None,
        msaa: None,
        sample_count: aa_sample_count(AaConfig::Area),
        fonts: None,
        swash: None,
        font_ids: HashMap::new(),
        atlas: None,
        color_atlas: None,
        glyph_pipeline: None,
        color_pipeline: None,
        image_cache: HashMap::new(),
        image_pipeline: None,
        shadow_pipeline: None,
    }
}

impl Renderer {
    pub(crate) fn render_to_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        target: &wgpu::TextureView,
        params: &RenderParams,
    ) -> Result<(), String> {
        let format = *SURFACE_FORMAT.get().ok_or("pebbles(wgpu): surface format not yet known")?;
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

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_offscreen(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        target: &wgpu::TextureView,
        base_color: peniko::Color,
        size: (u32, u32),
        format: wgpu::TextureFormat,
    ) -> Result<(), String> {
        let size = (size.0.max(1), size.1.max(1));
        if self.pipeline.as_ref().is_none_or(|p| p.0 != format) {
            self.pipeline = Some(make_pipeline(device, format, self.sample_count));
        }
        let (.., pipeline, uniform, bind_group) = self.pipeline.as_ref().expect("pipeline just initialized");
        queue.write_buffer(uniform, 0, bytemuck::cast_slice(&[size.0 as f32, size.1 as f32, 0.0, 0.0]));
        // Recreate the multisampled color target when the format or size changes.
        if self.msaa.as_ref().is_none_or(|m| m.0 != format || m.1 != size.0 || m.2 != size.1) {
            let texture = create_msaa(device, format, size.0, size.1, self.sample_count);
            self.msaa = Some((format, size.0, size.1, texture));
        }
        let msaa = &self.msaa.as_ref().expect("msaa just initialized").3;

        let mesh = pebbles_render::paint::wgpu_backend::mesh(scene);
        let [r, g, b, a] = base_color.components;
        let mut encoder = device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("pebbles.wgpu.render") });
        {
            let msaa_view = msaa.create_view(&wgpu::TextureViewDescriptor::default());
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("pebbles.wgpu.pass"),
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
                    label: Some("pebbles.wgpu.vertices"),
                    contents: bytemuck::cast_slice(&mesh.solid.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                });
                let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("pebbles.wgpu.indices"),
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
                self.draw_glyphs(device, queue, &mut pass, &mesh.glyphs, format, size.0, size.1);
            }
            if !mesh.shadows.is_empty() {
                self.draw_shadows(device, queue, &mut pass, &mesh.shadows, format, size.0, size.1);
            }
            if !mesh.images.is_empty() {
                self.draw_images(device, queue, &mut pass, &mesh.images, scene, format, size.0, size.1);
            }
        }
        queue.submit([encoder.finish()]);
        Ok(())
    }

    /// Rasterize the frame's glyphs into the coverage atlas and draw them as tinted
    /// textured quads, one `set_scissor_rect` + draw per run.
    #[allow(clippy::too_many_arguments)]
    fn draw_glyphs(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        runs: &[GlyphRun],
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) {
        let (coverage, color) = self.build_glyphs(device, queue, runs);
        if !coverage.is_empty() {
            let atlas = self.atlas.as_ref().expect("atlas built by build_glyphs");
            if self.glyph_pipeline.as_ref().is_none_or(|p| p.0 != format) {
                let gp = make_glyph_pipeline(device, format, atlas, self.sample_count, false);
                self.glyph_pipeline = Some((format, gp));
            }
            let (_, gp) = self.glyph_pipeline.as_ref().expect("glyph pipeline just built");
            draw_glyph_set(device, queue, pass, &coverage, gp, width, height);
        }
        if !color.is_empty() {
            let atlas = self.color_atlas.as_ref().expect("color atlas built by build_glyphs");
            if self.color_pipeline.as_ref().is_none_or(|p| p.0 != format) {
                let gp = make_glyph_pipeline(device, format, atlas, self.sample_count, true);
                self.color_pipeline = Some((format, gp));
            }
            let (_, gp) = self.color_pipeline.as_ref().expect("color pipeline just built");
            draw_glyph_set(device, queue, pass, &color, gp, width, height);
        }
    }

    /// Rasterize each glyph once (cached), pack it into the coverage or color atlas,
    /// and emit the quad geometry, batched per scissor rect.
    fn build_glyphs(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        runs: &[GlyphRun],
    ) -> (GlyphDraw, GlyphDraw) {
        let mut coverage = GlyphDraw::new();
        let mut color = GlyphDraw::new();
        if self.fonts.is_none() {
            self.fonts = Some(FontSystem::new_with_fonts(Vec::<fontdb::Source>::new()));
        }
        if self.swash.is_none() {
            self.swash = Some(SwashCache::new());
        }
        if self.atlas.is_none() {
            self.atlas = Some(Atlas::new(device, ATLAS_SIZE, wgpu::TextureFormat::R8Unorm));
        }
        if self.color_atlas.is_none() {
            self.color_atlas = Some(Atlas::new(device, ATLAS_SIZE, wgpu::TextureFormat::Rgba8Unorm));
        }
        for run in runs {
            let Some(font_id) = self.font_id(&run.font) else { continue };
            let flags = if run.synthesize_italic {
                cosmic_text::CacheKeyFlags::FAKE_ITALIC
            } else {
                cosmic_text::CacheKeyFlags::empty()
            };
            for glyph in &run.glyphs {
                // Snap the baseline to an integer pixel (Y-axis hinting) so
                // horizontal stems render crisp — matching cosmic-text's own
                // renderer (`math::truncf` on the physical Y). X stays subpixel.
                let (key, x, y) = CacheKey::new(
                    font_id,
                    glyph.id as u16,
                    run.font_size,
                    (glyph.x, glyph.y.trunc()),
                    fontdb::Weight(run.font_weight),
                    flags,
                );
                let Some(image) = self
                    .swash
                    .as_mut()
                    .expect("swash initialized")
                    .get_image(self.fonts.as_mut().expect("fonts initialized"), key)
                    .clone()
                else {
                    continue;
                };
                let (w, h) = (image.placement.width, image.placement.height);
                if w == 0 || h == 0 {
                    continue;
                }
                match image.content {
                    SwashContent::Mask => {
                        let Some(region) = self.atlas.as_mut().expect("atlas initialized").pack(w, h) else {
                            continue;
                        };
                        queue.write_texture(
                            wgpu::TexelCopyTextureInfo {
                                texture: &self.atlas.as_ref().expect("atlas initialized").texture,
                                mip_level: 0,
                                origin: wgpu::Origin3d { x: region.x, y: region.y, z: 0 },
                                aspect: wgpu::TextureAspect::All,
                            },
                            &image.data,
                            wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(w),
                                rows_per_image: Some(h),
                            },
                            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                        );
                        let quad = glyph_quad(
                            region,
                            w,
                            h,
                            x,
                            y,
                            image.placement.left,
                            image.placement.top,
                            run.color,
                        );
                        coverage.push_quad(quad, run.scissor);
                    }
                    SwashContent::Color => {
                        let Some(region) =
                            self.color_atlas.as_mut().expect("color atlas initialized").pack(w, h)
                        else {
                            continue;
                        };
                        queue.write_texture(
                            wgpu::TexelCopyTextureInfo {
                                texture: &self.color_atlas.as_ref().expect("color atlas initialized").texture,
                                mip_level: 0,
                                origin: wgpu::Origin3d { x: region.x, y: region.y, z: 0 },
                                aspect: wgpu::TextureAspect::All,
                            },
                            &image.data,
                            wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(w * 4),
                                rows_per_image: Some(h),
                            },
                            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                        );
                        // Color glyphs are already colored; tint is white (ignored).
                        let quad = glyph_quad(
                            region,
                            w,
                            h,
                            x,
                            y,
                            image.placement.left,
                            image.placement.top,
                            [1.0, 1.0, 1.0, 1.0],
                        );
                        color.push_quad(quad, run.scissor);
                    }
                    // LCD subpixel masks are for RGB subpixel rendering; grayscale AA is used instead.
                    SwashContent::SubpixelMask => continue,
                }
            }
        }
        (coverage, color)
    }

    /// Resolve a font's `fontdb::ID` (registering its bytes on first sight) so the
    /// glyph cache keys reference the host's own font database.
    fn font_id(&mut self, font: &peniko::FontData) -> Option<fontdb::ID> {
        let key = (font.data.id(), font.index);
        if let Some(&id) = self.font_ids.get(&key) {
            return Some(id);
        }
        let bytes = font.data.data().to_vec();
        let source = fontdb::Source::Binary(std::sync::Arc::new(bytes));
        let ids = self.fonts.get_or_insert_with(FontSystem::new).db_mut().load_font_source(source);
        let id = ids.get(font.index as usize).copied().or_else(|| ids.first().copied())?;
        self.font_ids.insert(key, id);
        Some(id)
    }

    /// Draw the analytic rounded-rect shadows (one draw call — shadows are unclipped
    /// elevation shadows).
    #[allow(clippy::too_many_arguments)]
    fn draw_shadows(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        shadows: &[ShadowVertex],
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) {
        if self.shadow_pipeline.as_ref().is_none_or(|p| p.0 != format) {
            self.shadow_pipeline = Some((format, make_shadow_pipeline(device, format, self.sample_count)));
        }
        let (_, sp) = self.shadow_pipeline.as_ref().expect("shadow pipeline just built");
        queue.write_buffer(&sp.uniform, 0, bytemuck::cast_slice(&[width as f32, height as f32, 0.0, 0.0]));
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("pebbles.wgpu.shadow.vertices"),
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
    fn draw_images(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pass: &mut wgpu::RenderPass<'_>,
        images: &[ImageQuad],
        scene: &Scene,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) {
        let uploads = pebbles_render::paint::wgpu_backend::image_uploads(scene);
        if uploads.is_empty() {
            return;
        }
        if self.image_pipeline.as_ref().is_none_or(|p| p.0 != format) {
            self.image_pipeline = Some((format, make_image_pipeline(device, format, self.sample_count)));
        }
        let (_, ip) = self.image_pipeline.as_ref().expect("image pipeline just built");
        queue.write_buffer(&ip.uniform, 0, bytemuck::cast_slice(&[width as f32, height as f32, 0.0, 0.0]));
        for quad in images {
            let Some(upload) = uploads.get(quad.image_index as usize) else { continue };
            let cache = &mut self.image_cache;
            let texture = cache.entry(upload.id).or_insert_with(|| {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("pebbles.wgpu.image"),
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
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&ip.sampler),
                    },
                ],
            });
            let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("pebbles.wgpu.image.vertices"),
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
}

/// An index range into the glyph vertex/index buffers plus the scissor rect active
/// while drawing it.
struct GlyphBatch {
    range: std::ops::Range<u32>,
    scissor: Option<Scissor>,
}

/// Vertex/index geometry for one glyph pipeline (coverage or color), batched by
/// scissor rect.
struct GlyphDraw {
    vertices: Vec<GlyphVertex>,
    indices: Vec<u32>,
    batches: Vec<GlyphBatch>,
}

impl GlyphDraw {
    fn new() -> Self {
        Self { vertices: Vec::new(), indices: Vec::new(), batches: Vec::new() }
    }

    fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Append one quad, merging it into the current batch when the scissor rect is
    /// unchanged (so consecutive same-clip glyphs become a single draw call).
    fn push_quad(&mut self, quad: [GlyphVertex; 4], scissor: Option<Scissor>) {
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

/// Clamp a scissor rect (physical-pixel space) to the render-target bounds. `None`
/// means "no clip" — the full target.
fn scissor_params(scissor: Option<Scissor>, width: u32, height: u32) -> (u32, u32, u32, u32) {
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

/// Build the four vertices of a glyph quad: physical-pixel position (from the integer
/// pen position + the bitmap placement) and atlas UVs.
#[allow(clippy::too_many_arguments)]
fn glyph_quad(
    region: AtlasRegion,
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

/// Draw one glyph set (coverage or color) with its pipeline: upload the vertex/index
/// buffers, then draw each batch under its scissor rect.
fn draw_glyph_set(
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
        label: Some("pebbles.wgpu.glyph.vertices"),
        contents: bytemuck::cast_slice(&draw.vertices),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("pebbles.wgpu.glyph.indices"),
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

/// A glyph atlas with a simple shelf packer: `R8Unorm` for coverage glyphs and
/// `Rgba8Unorm` for color (emoji) glyphs. Glyphs are cached by [`CacheKey`] across
/// frames; the texture is fixed-size for v1 (growth/eviction is a follow-up).
struct Atlas {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    size: u32,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
}

struct AtlasRegion {
    x: u32,
    y: u32,
}

impl Atlas {
    fn new(device: &wgpu::Device, size: u32, format: wgpu::TextureFormat) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pebbles.wgpu.glyph-atlas"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("pebbles.wgpu.glyph-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self { texture, view, sampler, size, cursor_x: 1, cursor_y: 1, row_height: 0 }
    }

    /// Pack a `w`×`h` glyph into the atlas (with 1px padding), returning its region.
    /// Returns `None` when the atlas is full (the glyph is skipped for v1).
    fn pack(&mut self, w: u32, h: u32) -> Option<AtlasRegion> {
        let (w_pad, h_pad) = (w + 2, h + 2);
        if self.cursor_x + w_pad > self.size {
            self.cursor_x = 1;
            self.cursor_y += self.row_height;
            self.row_height = 0;
        }
        if self.cursor_y + h_pad > self.size {
            return None;
        }
        let region = AtlasRegion { x: self.cursor_x, y: self.cursor_y };
        self.cursor_x += w_pad;
        self.row_height = self.row_height.max(h_pad);
        Some(region)
    }
}

/// The glyph pipeline: a textured quad sampling the coverage atlas and tinted by the
/// run color (premultiplied alpha), with its own viewport-size uniform.
struct GlyphPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
}

fn make_glyph_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    atlas: &Atlas,
    sample_count: u32,
    color: bool,
) -> GlyphPipeline {
    // Coverage glyphs tint the R8 coverage by the run color; color glyphs sample the
    // Rgba8 bitmap directly and premultiply its (straight) alpha.
    let fragment = if color {
        "let c = textureSample(atlas, atlas_sampler, i.uv); return vec4(c.rgb * c.a, c.a);"
    } else {
        "let coverage = textureSample(atlas, atlas_sampler, i.uv).r; return vec4(i.c.rgb * coverage, i.c.a * coverage);"
    };
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("pebbles.wgpu.glyph"),
        source: wgpu::ShaderSource::Wgsl(
            format!(
                "@group(0) @binding(0) var<uniform> size: vec4<f32>;
                 @group(0) @binding(1) var atlas: texture_2d<f32>;
                 @group(0) @binding(2) var atlas_sampler: sampler;
                 struct I {{ @location(0) p: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) c: vec4<f32> }}
                 struct O {{ @builtin(position) p: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) c: vec4<f32> }}
                 @vertex fn vs(i: I) -> O {{
                     var o: O;
                     o.p = vec4((i.p.x / size.x) * 2.0 - 1.0, 1.0 - (i.p.y / size.y) * 2.0, 0.0, 1.0);
                     o.uv = i.uv;
                     o.c = i.c;
                     return o;
                 }}
                 @fragment fn fs(i: O) -> @location(0) vec4<f32> {{
                     {fragment}
                 }}"
            )
            .into(),
        ),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas.view) },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&atlas.sampler) },
        ],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("pebbles.wgpu.glyph"),
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: size_of::<GlyphVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 8,
                        shader_location: 1,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: 16,
                        shader_location: 2,
                    },
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState { count: sample_count, ..Default::default() },
        multiview_mask: None,
        cache: None,
    });
    GlyphPipeline { pipeline, bind_group, uniform }
}

fn create_msaa(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    sample_count: u32,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.wgpu.msaa"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

fn make_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    sample_count: u32,
) -> (wgpu::TextureFormat, wgpu::RenderPipeline, wgpu::Buffer, wgpu::BindGroup) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("pebbles.wgpu.solid"), source: wgpu::ShaderSource::Wgsl("@group(0) @binding(0) var<uniform> size: vec4<f32>; struct I { @location(0) p: vec2<f32>, @location(1) c: vec4<f32> }; struct O { @builtin(position) p: vec4<f32>, @location(0) c: vec4<f32> }; @vertex fn vs(i:I)->O { var o:O; o.p=vec4((i.p.x/size.x)*2.0-1.0,1.0-(i.p.y/size.y)*2.0,0.0,1.0); o.c=i.c; return o; } @fragment fn fs(i:O)->@location(0) vec4<f32>{return i.c;}".into()) });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("pebbles.wgpu.solid"),
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: 8,
                        shader_location: 1,
                    },
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState { count: sample_count, ..Default::default() },
        multiview_mask: None,
        cache: None,
    });
    (format, pipeline, uniform, bind_group)
}

/// The image pipeline: a textured quad sampling an uploaded `Rgba8` image and
/// premultiplying its (straight) alpha, with a per-image bind group.
struct ImagePipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
}

fn make_image_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    sample_count: u32,
) -> ImagePipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("pebbles.wgpu.image"),
        source: wgpu::ShaderSource::Wgsl(
            "@group(0) @binding(0) var<uniform> size: vec4<f32>;
             @group(0) @binding(1) var image: texture_2d<f32>;
             @group(0) @binding(2) var image_sampler: sampler;
             struct I { @location(0) p: vec2<f32>, @location(1) uv: vec2<f32> }
             struct O { @builtin(position) p: vec4<f32>, @location(0) uv: vec2<f32> }
             @vertex fn vs(i: I) -> O {
                 var o: O;
                 o.p = vec4((i.p.x / size.x) * 2.0 - 1.0, 1.0 - (i.p.y / size.y) * 2.0, 0.0, 1.0);
                 o.uv = i.uv;
                 return o;
             }
             @fragment fn fs(i: O) -> @location(0) vec4<f32> {
                 let c = textureSample(image, image_sampler, i.uv);
                 return vec4(c.rgb * c.a, c.a);
             }"
            .into(),
        ),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("pebbles.wgpu.image-sampler"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("pebbles.wgpu.image.indices"),
        contents: bytemuck::cast_slice(&[0u32, 1, 2, 0, 2, 3]),
        usage: wgpu::BufferUsages::INDEX,
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("pebbles.wgpu.image"),
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: size_of::<ImageVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 8,
                        shader_location: 1,
                    },
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState { count: sample_count, ..Default::default() },
        multiview_mask: None,
        cache: None,
    });
    ImagePipeline { pipeline, layout, sampler, uniform, index_buffer }
}

/// The shadow pipeline: an analytic rounded-rect gaussian shadow (Evan-Wallace-style
/// `erf` coverage of the rounded-rect SDF).
struct ShadowPipeline {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
}

fn make_shadow_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    sample_count: u32,
) -> ShadowPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("pebbles.wgpu.shadow"),
        source: wgpu::ShaderSource::Wgsl(
            "@group(0) @binding(0) var<uniform> size: vec4<f32>;
             struct I {
                 @location(0) p: vec2<f32>,
                 @location(1) rect: vec4<f32>,
                 @location(2) radius: f32,
                 @location(3) sigma: f32,
                 @location(4) color: vec4<f32>,
             }
             struct O {
                 @builtin(position) clip: vec4<f32>,
                 @location(0) p: vec2<f32>,
                 @location(1) rect: vec4<f32>,
                 @location(2) radius: f32,
                 @location(3) sigma: f32,
                 @location(4) color: vec4<f32>,
             }
             @vertex fn vs(i: I) -> O {
                 var o: O;
                 o.clip = vec4((i.p.x / size.x) * 2.0 - 1.0, 1.0 - (i.p.y / size.y) * 2.0, 0.0, 1.0);
                 o.p = i.p;
                 o.rect = i.rect;
                 o.radius = i.radius;
                 o.sigma = i.sigma;
                 o.color = i.color;
                 return o;
             }
             fn erf(x: f32) -> f32 {
                 let s = select(-1.0, 1.0, x >= 0.0);
                 let a = abs(x);
                 let t = 1.0 / (1.0 + 0.3275911 * a);
                 let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * exp(-a * a);
                 return s * y;
             }
             @fragment fn fs(i: O) -> @location(0) vec4<f32> {
                 let center = vec2<f32>((i.rect.x + i.rect.z) * 0.5, (i.rect.y + i.rect.w) * 0.5);
                 let half = vec2<f32>((i.rect.z - i.rect.x) * 0.5, (i.rect.w - i.rect.y) * 0.5);
                 let q = abs(i.p - center) - half + vec2<f32>(i.radius, i.radius);
                 let sdf = length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - i.radius;
                 let coverage = 0.5 * (1.0 + erf(-sdf / (i.sigma * 1.41421356)));
                 return vec4(i.color.rgb * coverage, i.color.a * coverage);
             }"
            .into(),
        ),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let uniform = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() }],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("pebbles.wgpu.shadow"),
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: size_of::<ShadowVertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: 8,
                        shader_location: 1,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32,
                        offset: 24,
                        shader_location: 2,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32,
                        offset: 28,
                        shader_location: 3,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: 32,
                        shader_location: 4,
                    },
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState { count: sample_count, ..Default::default() },
        multiview_mask: None,
        cache: None,
    });
    ShadowPipeline { pipeline, bind_group, uniform }
}
