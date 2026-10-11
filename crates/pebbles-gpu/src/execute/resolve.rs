//! Glyph resolution: rasterize each glyph once (cached) into the coverage/color atlas
//! and emit the quad geometry for one glyph command.

use cosmic_text::{CacheKey, CacheKeyFlags, FontSystem, SwashCache, SwashContent};

use super::super::prepare::GlyphRun;
use super::super::prepare::num::index_u32;
use super::super::renderer::Renderer;
use super::resources::{ATLAS_SIZE, Atlas};
use crate::gpu_types::GlyphVertex;
use crate::raster::font_id;

/// Vertex/index geometry for one glyph pipeline (coverage or color).
pub(crate) struct GlyphGeometry {
    pub(crate) vertices: Vec<GlyphVertex>,
    pub(crate) indices: Vec<u32>,
}

impl GlyphGeometry {
    fn new() -> Self {
        Self { vertices: Vec::new(), indices: Vec::new() }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }
    fn push_quad(&mut self, quad: [GlyphVertex; 4]) {
        let base = index_u32(self.vertices.len());
        self.vertices.extend_from_slice(&quad);
        self.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// Rasterize each glyph once (cached), pack it into the coverage or color atlas, and emit
/// the quad geometry (coverage and color separately).
pub(crate) fn build_glyphs(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    runs: &[GlyphRun],
) -> (GlyphGeometry, GlyphGeometry) {
    let mut coverage = GlyphGeometry::new();
    let mut color = GlyphGeometry::new();
    if renderer.fonts.is_none() {
        renderer.fonts = Some(FontSystem::new_with_fonts(Vec::<fontdb::Source>::new()));
    }
    if renderer.swash.is_none() {
        renderer.swash = Some(SwashCache::new());
    }
    if renderer.atlas.is_none() {
        renderer.atlas = Some(Atlas::new(device, ATLAS_SIZE, wgpu::TextureFormat::R8Unorm));
    }
    if renderer.color_atlas.is_none() {
        renderer.color_atlas = Some(Atlas::new(device, ATLAS_SIZE, wgpu::TextureFormat::Rgba8Unorm));
    }
    for run in runs {
        let Some(font_id) = font_id(renderer, &run.font) else { continue };
        let flags = if run.synthesize_italic { CacheKeyFlags::FAKE_ITALIC } else { CacheKeyFlags::empty() };
        for glyph in &run.glyphs {
            let Ok(glyph_id) = u16::try_from(glyph.id) else { continue };
            let (key, x, y) = CacheKey::new(
                font_id,
                glyph_id,
                run.font_size,
                (glyph.x, glyph.y.trunc()),
                fontdb::Weight(run.font_weight),
                flags,
            );
            let Some(image) = renderer
                .swash
                .as_mut()
                .expect("swash initialized")
                .get_image(renderer.fonts.as_mut().expect("fonts initialized"), key)
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
                    let Some(region) = renderer.atlas.as_mut().expect("atlas initialized").pack(w, h) else {
                        continue;
                    };
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &renderer.atlas.as_ref().expect("atlas initialized").texture,
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
                    let quad =
                        glyph_quad(region, w, h, x, y, image.placement.left, image.placement.top, run.color);
                    coverage.push_quad(quad);
                }
                SwashContent::Color => {
                    let Some(region) =
                        renderer.color_atlas.as_mut().expect("color atlas initialized").pack(w, h)
                    else {
                        continue;
                    };
                    queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &renderer.color_atlas.as_ref().expect("color atlas initialized").texture,
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
                    color.push_quad(quad);
                }
                SwashContent::SubpixelMask => continue,
            }
        }
    }
    (coverage, color)
}

/// Build the four vertices of a glyph quad: physical-pixel position + atlas UVs.
#[allow(clippy::too_many_arguments)]
fn glyph_quad(
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
