//! Glyph resolution: rasterize each glyph once (cached) into the coverage/color atlas
//! and emit the quad geometry.

use cosmic_text::{CacheKey, CacheKeyFlags, FontSystem, SwashCache, SwashContent};

use super::super::prepare::GlyphRun;
use super::super::renderer::Renderer;
use super::encode::{GlyphDraw, glyph_quad};
use super::resources::{ATLAS_SIZE, Atlas};
use crate::raster::font_id;

/// Rasterize each glyph once (cached), pack it into the coverage or color atlas,
/// and emit the quad geometry, batched per scissor rect.
pub(crate) fn build_glyphs(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    runs: &[GlyphRun],
) -> (GlyphDraw, GlyphDraw) {
    let mut coverage = GlyphDraw::new();
    let mut color = GlyphDraw::new();
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
            let (key, x, y) = CacheKey::new(
                font_id,
                glyph.id as u16,
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
                    coverage.push_quad(quad, run.scissor);
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
                    color.push_quad(quad, run.scissor);
                }
                SwashContent::SubpixelMask => continue,
            }
        }
    }
    (coverage, color)
}
