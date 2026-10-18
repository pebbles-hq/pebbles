//! The glyph atlas: one `R8Unorm` mask page and one `Rgba8Unorm` (premultiplied) color page,
//! each a 2-layer array texture (WebGL2 rule W1; only layer 0 is allocated until M01). Texels
//! are uploaded only when a glyph is inserted — never on a hit (10 §5).

use super::atlas_alloc::{
    AtlasAllocator, AtlasFull, AtlasKind, AtlasSlot, GLYPH_GUTTER_PX, GlyphKey, MaskKey,
};
use crate::prepare::path_masks::MaskSource;
use crate::raster::coverage::{CoverageRasterizer, MaskBuf};
use crate::raster::swash::{GlyphRaster, RasterContent, RasterRequest};

/// Page side cap (10 §5.4); the device limit may lower it.
pub(crate) const GLYPH_ATLAS_SIDE: u32 = 2048;
/// Physical array layers per page (≥ 2, never a multiple of 6 — W1/W2).
const PHYSICAL_LAYERS: u32 = 2;

/// One atlas page: the texture, its array view and the allocator.
struct Page {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    alloc: AtlasAllocator,
    bytes_per_pixel: u32,
}

impl Page {
    fn new(device: &wgpu::Device, kind: AtlasKind, side: u32) -> Self {
        let (format, bytes_per_pixel, label) = match kind {
            AtlasKind::Mask => (wgpu::TextureFormat::R8Unorm, 1, "pebbles.gpu.atlas.mask"),
            AtlasKind::Color => (wgpu::TextureFormat::Rgba8Unorm, 4, "pebbles.gpu.atlas.color"),
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: side, height: side, depth_or_array_layers: PHYSICAL_LAYERS },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some(label),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            array_layer_count: Some(PHYSICAL_LAYERS),
            ..Default::default()
        });
        Self { texture, view, alloc: AtlasAllocator::new(kind, side), bytes_per_pixel }
    }

    /// Upload a padded (zero gutter) copy of `data` (`w`×`h`, tightly packed) at `slot`.
    fn upload(
        &self,
        queue: &wgpu::Queue,
        staging: &mut Vec<u8>,
        slot: AtlasSlot,
        data: &[u8],
        premultiply: bool,
    ) {
        let bpp = self.bytes_per_pixel as usize;
        let (w, h) = (usize::from(slot.w), usize::from(slot.h));
        let g = GLYPH_GUTTER_PX as usize;
        let (pw, ph) = (w + 2 * g, h + 2 * g);
        staging.clear();
        staging.resize(pw * ph * bpp, 0);
        for row in 0..h {
            let src = &data[row * w * bpp..(row + 1) * w * bpp];
            let dst = ((row + g) * pw + g) * bpp;
            staging[dst..dst + w * bpp].copy_from_slice(src);
        }
        if premultiply {
            for px in staging.as_chunks_mut::<4>().0 {
                let a = u16::from(px[3]);
                for c in &mut px[..3] {
                    *c = u8::try_from((u16::from(*c) * a + 127) / 255).unwrap_or(u8::MAX);
                }
            }
        }
        let gutter = u32::from(slot.x) - GLYPH_GUTTER_PX;
        let top = u32::from(slot.y) - GLYPH_GUTTER_PX;
        let (pw, ph) = (u32::from(slot.w) + 2 * GLYPH_GUTTER_PX, u32::from(slot.h) + 2 * GLYPH_GUTTER_PX);
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: gutter, y: top, z: u32::from(slot.layer) },
                aspect: wgpu::TextureAspect::All,
            },
            staging,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pw * self.bytes_per_pixel),
                rows_per_image: None,
            },
            wgpu::Extent3d { width: pw, height: ph, depth_or_array_layers: 1 },
        );
    }
}

/// Outcome of looking up one glyph.
pub(crate) enum GlyphLookup {
    /// Resident (hit or freshly inserted).
    Slot(AtlasSlot),
    /// Empty bitmap (a space) or no representation: draw nothing.
    Empty,
    /// No room this frame (counted as `AtlasFull`).
    Full,
    /// swash returned a subpixel mask (counted as `GlyphContent`).
    Unsupported,
}

/// Both atlas pages, the rasterizer, and the bind group that samples them.
pub(crate) struct GlyphAtlas {
    mask: Page,
    color: Page,
    raster: GlyphRaster,
    paths: CoverageRasterizer,
    mask_buf: MaskBuf,
    staging: Vec<u8>,
    pub(crate) bind_group: wgpu::BindGroup,
}

impl GlyphAtlas {
    pub(crate) fn new(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, path_mask_bytes: u64) -> Self {
        let side = GLYPH_ATLAS_SIDE.min(device.limits().max_texture_dimension_2d);
        let mut mask = Page::new(device, AtlasKind::Mask, side);
        mask.alloc.set_path_budget(path_mask_bytes);
        let color = Page::new(device, AtlasKind::Color, side);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("pebbles.gpu.atlas.sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("pebbles.gpu.bind.glyph"),
            layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&mask.view) },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&color.view),
                },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        Self {
            mask,
            color,
            raster: GlyphRaster::default(),
            paths: CoverageRasterizer::default(),
            mask_buf: MaskBuf::default(),
            staging: Vec::new(),
            bind_group,
        }
    }

    pub(crate) fn begin_frame(&mut self) {
        self.mask.alloc.begin_frame();
        self.color.alloc.begin_frame();
    }

    pub(crate) fn end_frame(&mut self) {
        self.raster.end_frame();
    }

    /// Find `key`, rasterizing and uploading it on a miss. Returns the lookup and whether a
    /// glyph was rasterized.
    pub(crate) fn lookup(
        &mut self,
        queue: &wgpu::Queue,
        key: GlyphKey,
        req: &RasterRequest<'_>,
    ) -> (GlyphLookup, bool) {
        // A key lives in exactly one page; check both.
        for page in [&mut self.mask, &mut self.color] {
            if let Some(hit) = page.alloc.get(key) {
                return (hit.map_or(GlyphLookup::Empty, GlyphLookup::Slot), false);
            }
        }
        let Some(glyph) = self.raster.rasterize(req) else {
            self.mask.alloc.insert_empty(key);
            return (GlyphLookup::Empty, true);
        };
        let page = match glyph.content {
            RasterContent::Mask => &mut self.mask,
            RasterContent::Color => &mut self.color,
            RasterContent::Subpixel => return (GlyphLookup::Unsupported, true),
        };
        if glyph.width == 0 || glyph.height == 0 {
            page.alloc.insert_empty(key);
            return (GlyphLookup::Empty, true);
        }
        match page.alloc.insert(key, glyph.width, glyph.height, glyph.left, glyph.top) {
            Ok(slot) => {
                let premultiply = glyph.content == RasterContent::Color && !glyph.premultiplied;
                page.upload(queue, &mut self.staging, slot, glyph.data, premultiply);
                (GlyphLookup::Slot(slot), true)
            }
            Err(AtlasFull) => (GlyphLookup::Full, true),
        }
    }

    /// Find a Tier B path mask, rasterizing (R14) and uploading it on a miss. The slot's
    /// `left`/`top` place it relative to the request origin like a glyph (`x + left`, `y − top`).
    pub(crate) fn lookup_path(
        &mut self,
        queue: &wgpu::Queue,
        key: MaskKey,
        src: &MaskSource,
    ) -> (GlyphLookup, bool) {
        if let Some(hit) = self.mask.alloc.get(key) {
            return (hit.map_or(GlyphLookup::Empty, GlyphLookup::Slot), false);
        }
        let outline;
        let path = match &src.stroke {
            None => &src.path,
            Some(style) => {
                let tol = crate::prepare::path_masks::GEOMETRY_TOL_PX / src.device.spectral_norm().max(1e-9);
                outline = kurbo::stroke(
                    src.path.elements().iter().copied(),
                    style,
                    &kurbo::StrokeOpts::default(),
                    tol,
                );
                &outline
            }
        };
        let Some(placement) = rasterize_anywhere(&mut self.paths, path, src, &mut self.mask_buf) else {
            self.mask.alloc.insert_empty(key);
            return (GlyphLookup::Empty, true);
        };
        let (Ok(left), Ok(top)) = (i16::try_from(placement.0), i16::try_from(-placement.1)) else {
            return (GlyphLookup::Unsupported, true);
        };
        match self.mask.alloc.insert(key, self.mask_buf.width, self.mask_buf.height, left.into(), top.into())
        {
            Ok(slot) => {
                self.mask.upload(queue, &mut self.staging, slot, &self.mask_buf.data, false);
                (GlyphLookup::Slot(slot), true)
            }
            Err(AtlasFull) => (GlyphLookup::Full, true),
        }
    }
}

/// Rasterize `path` under `src.device` wherever it lies: the mask is computed in a space shifted
/// by the floor of its device bounds, which are returned as `(left, top)`.
fn rasterize_anywhere(
    r: &mut CoverageRasterizer,
    path: &kurbo::BezPath,
    src: &MaskSource,
    out: &mut MaskBuf,
) -> Option<(i32, i32)> {
    use kurbo::Shape as _;
    let b = src.device.transform_rect_bbox(path.bounding_box());
    let (x0, y0) = (b.x0.floor(), b.y0.floor());
    let w = crate::prepare::num::ceil_px(b.x1 - x0, u32::from(u16::MAX))?;
    let h = crate::prepare::num::ceil_px(b.y1 - y0, u32::from(u16::MAX))?;
    let shift = kurbo::Affine::translate((-x0, -y0)) * src.device;
    let clip = crate::prepare::ScissorRect { x0: 0, y0: 0, x1: w, y1: h };
    let p = r.rasterize(path, shift, src.rule, clip, out)?;
    let left = i32::try_from(crate::prepare::num::to_u64_sat(x0.abs())).ok()?;
    let top = i32::try_from(crate::prepare::num::to_u64_sat(y0.abs())).ok()?;
    let (left, top) = (if x0 < 0.0 { -left } else { left }, if y0 < 0.0 { -top } else { top });
    Some((left + p.left, top + p.top))
}
