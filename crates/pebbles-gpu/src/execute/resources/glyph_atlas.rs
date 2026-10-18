//! The coverage atlas, GPU side (10 §5.7–§5.9; M01): one `R8Unorm` mask texture and one
//! `Rgba8Unorm` (premultiplied) color texture, each a D2 array with ≥ 2 layers (W1, W2), grown by
//! `AtlasTexture::sync` (a separate submit before the frame's uploads, rule O1). Misses are
//! rasterized during resolve and staged as padded bitmaps; `flush` syncs then uploads them.

use super::atlas_alloc::{
    AtlasKind, AtlasPages, AtlasSlot, CoverageAtlas, GLYPH_GUTTER_PX, GlyphKey, MaskKey,
};
use super::bind_groups::{GenBindGroup, refresh};
use crate::budgets::GpuBudgets;
use crate::prepare::path_masks::MaskSource;
use crate::raster::coverage::{CoverageRasterizer, MaskBuf};
use crate::raster::swash::{GlyphRaster, RasterContent, RasterRequest};
use crate::stats::FrameStats;

/// One pending `write_texture` (10 §5.3).
pub(crate) struct GlyphUpload {
    pub kind: AtlasKind,
    pub layer: u16,
    /// Top-left of the PADDED region (gutter included).
    pub x: u16,
    pub y: u16,
    /// Padded size.
    pub w: u16,
    pub h: u16,
    pub bytes: std::ops::Range<usize>,
}

/// The GPU texture of one kind's pages (10 §5.7).
pub(crate) struct AtlasTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    side: u32,
    /// Physical layers.
    layers: u32,
    /// The `AtlasPages::generation` this texture matches.
    pub generation: u64,
}

fn format_of(kind: AtlasKind) -> (wgpu::TextureFormat, u32, &'static str) {
    match kind {
        AtlasKind::Mask => (wgpu::TextureFormat::R8Unorm, 1, "pebbles.gpu.atlas.mask"),
        AtlasKind::Color => (wgpu::TextureFormat::Rgba8Unorm, 4, "pebbles.gpu.atlas.color"),
    }
}

impl AtlasTexture {
    fn create(
        device: &wgpu::Device,
        kind: AtlasKind,
        side: u32,
        layers: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
        let (format, _, label) = format_of(kind);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d { width: side, height: side, depth_or_array_layers: layers },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        // W3: explicit array dimension and layer count.
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some(label),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            array_layer_count: Some(layers),
            ..Default::default()
        });
        (texture, view)
    }

    fn new(device: &wgpu::Device, pages: &AtlasPages) -> Self {
        let (texture, view) = Self::create(device, pages.kind, pages.side, pages.physical_layers);
        Self { texture, view, side: pages.side, layers: pages.physical_layers, generation: pages.generation }
    }

    /// SYNC step 2a: recreate at the pages' size when it changed, copying every old layer one
    /// layer per copy (W4), and submit that copy immediately (O1). Returns whether it was recreated.
    pub(crate) fn sync(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pages: &AtlasPages,
        stats: &mut FrameStats,
    ) -> bool {
        if self.side == pages.side && self.layers == pages.physical_layers {
            self.generation = pages.generation;
            return false;
        }
        let (texture, view) = Self::create(device, pages.kind, pages.side, pages.physical_layers);
        stats.textures_created += 1;
        stats.texture_views_created += 1;
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("pebbles.gpu.encoder.atlas_grow"),
        });
        for l in 0..self.layers.min(pages.physical_layers) {
            let at = |t| wgpu::TexelCopyTextureInfo {
                texture: t,
                mip_level: 0,
                origin: wgpu::Origin3d { x: 0, y: 0, z: l },
                aspect: wgpu::TextureAspect::All,
            };
            enc.copy_texture_to_texture(
                at(&self.texture),
                at(&texture),
                wgpu::Extent3d { width: self.side, height: self.side, depth_or_array_layers: 1 },
            );
        }
        queue.submit([enc.finish()]);
        (self.texture, self.view, self.side, self.layers, self.generation) =
            (texture, view, pages.side, pages.physical_layers, pages.generation);
        true
    }
}

/// Outcome of looking up one entry.
pub(crate) enum GlyphLookup {
    /// Resident (hit or freshly inserted; texels land in this frame's flush).
    Slot(AtlasSlot),
    /// Empty bitmap (a space) or no representation: draw nothing.
    Empty,
    /// No room after the whole ladder (counted as `AtlasFull`).
    Full,
    /// swash returned a subpixel mask (counted as `GlyphContent`).
    Unsupported,
}

/// The coverage atlas: CPU residency + both textures + the rasterizers + the bind group.
pub(crate) struct GlyphAtlas {
    pub(crate) cpu: CoverageAtlas,
    mask: AtlasTexture,
    color: AtlasTexture,
    raster: GlyphRaster,
    paths: CoverageRasterizer,
    mask_buf: MaskBuf,
    uploads: Vec<GlyphUpload>,
    staging: Vec<u8>,
    sampler: wgpu::Sampler,
    bind_group: Option<GenBindGroup>,
}

impl GlyphAtlas {
    pub(crate) fn new(device: &wgpu::Device, budgets: &GpuBudgets) -> Self {
        let max_side = budgets.glyph_atlas_max_side.min(device.limits().max_texture_dimension_2d);
        let cpu = CoverageAtlas::new(
            max_side,
            budgets.glyph_mask_max_layers,
            budgets.glyph_color_max_layers,
            budgets.path_mask_bytes,
        );
        let mask = AtlasTexture::new(device, &cpu.mask);
        let color = AtlasTexture::new(device, &cpu.color);
        // 10 §3.5: glyph atlases sample { Linear, ClampToEdge, ClampToEdge }.
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("pebbles.gpu.atlas.sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            cpu,
            mask,
            color,
            raster: GlyphRaster::default(),
            paths: CoverageRasterizer::default(),
            mask_buf: MaskBuf::default(),
            uploads: Vec::new(),
            staging: Vec::new(),
            sampler,
            bind_group: None,
        }
    }

    pub(crate) fn begin_frame(&mut self) {
        self.cpu.begin_frame();
        self.uploads.clear();
        self.staging.clear();
    }

    /// Census fields owned by the glyph/coverage atlas (M05).
    #[cfg(debug_assertions)]
    pub(crate) fn census_into(&self, c: &mut crate::census::GpuCensus) {
        c.glyph_entries = self.cpu.len();
        c.glyph_mask_side = self.cpu.mask.side;
        c.glyph_mask_layers = self.cpu.mask.physical_layers;
        c.glyph_color_side = self.cpu.color.side;
        c.glyph_color_layers = self.cpu.color.physical_layers;
        c.raster_fonts = self.raster.font_count();
    }

    pub(crate) fn end_frame(&mut self, stats: &mut FrameStats) {
        self.cpu.end_frame(stats);
        self.raster.end_frame();
    }

    /// The bind group of the current textures (after `flush`).
    pub(crate) fn bind_group(&self) -> Option<&wgpu::BindGroup> {
        self.bind_group.as_ref().map(|g| &g.group)
    }
}

/// Premultiply straight-alpha RGBA8 in place: `c' = (c·a + 127) / 255` (M14; only
/// `Source::ColorBitmap` rasters are straight — COLR output is already premultiplied). Applied
/// by the rasterizer before any resize (`raster::swash::color_upload_bytes`, R23).
pub(crate) fn premultiply_rgba8_in_place(px: &mut [u8]) {
    for px in px.as_chunks_mut::<4>().0 {
        let a = u16::from(px[3]);
        for c in &mut px[..3] {
            *c = u8::try_from((u16::from(*c) * a + 127) / 255).unwrap_or(u8::MAX);
        }
    }
}

/// Stage a padded (zero gutter) copy of `data` (`w`×`h`, tight) for `slot` (§5.8): written
/// straight from the rasterizer's scratch into `staging`, no per-glyph allocation.
fn stage(staging: &mut Vec<u8>, uploads: &mut Vec<GlyphUpload>, slot: AtlasSlot, data: &[u8]) {
    {
        let bpp = format_of(slot.kind).1 as usize;
        let (w, h) = (usize::from(slot.w), usize::from(slot.h));
        let g = GLYPH_GUTTER_PX as usize;
        let (pw, ph) = (w + 2 * g, h + 2 * g);
        let start = staging.len();
        staging.resize(start + pw * ph * bpp, 0);
        for row in 0..h {
            let Some(src) = data.get(row * w * bpp..(row + 1) * w * bpp) else { break };
            let dst = start + ((row + g) * pw + g) * bpp;
            staging[dst..dst + w * bpp].copy_from_slice(src);
        }
        let gutter = u16::try_from(GLYPH_GUTTER_PX).unwrap_or(1);
        let to16 = |v: usize| u16::try_from(v).unwrap_or(u16::MAX);
        uploads.push(GlyphUpload {
            kind: slot.kind,
            layer: slot.layer,
            x: slot.x - gutter,
            y: slot.y - gutter,
            w: to16(pw),
            h: to16(ph),
            bytes: start..staging.len(),
        });
    }
}

impl GlyphAtlas {
    /// Find glyph `key`, rasterizing and staging it on a miss. Returns the lookup and whether a
    /// glyph was rasterized.
    pub(crate) fn lookup(
        &mut self,
        key: GlyphKey,
        req: &RasterRequest<'_>,
        stats: &mut FrameStats,
    ) -> (GlyphLookup, bool) {
        if let Some(hit) = self.cpu.get(key) {
            return (hit.map_or(GlyphLookup::Empty, GlyphLookup::Slot), false);
        }
        let Some(glyph) = self.raster.rasterize(req) else {
            self.cpu.insert_empty(key);
            return (GlyphLookup::Empty, true);
        };
        let atlas = match glyph.content {
            RasterContent::Mask => AtlasKind::Mask,
            RasterContent::Color => AtlasKind::Color,
        };
        if glyph.width == 0 || glyph.height == 0 {
            self.cpu.insert_empty(key);
            return (GlyphLookup::Empty, true);
        }
        let (w, h, left, top) = (glyph.width, glyph.height, glyph.left, glyph.top);
        match self.cpu.insert(key, atlas, w, h, left, top, stats) {
            Ok(slot) => {
                // The rasterizer already premultiplied straight strikes (R23).
                stage(&mut self.staging, &mut self.uploads, slot, glyph.data);
                let slot = match glyph.strike_ppem {
                    0 => slot,
                    ppem => self.cpu.set_strike_ppem(key, ppem).unwrap_or(slot),
                };
                (GlyphLookup::Slot(slot), true)
            }
            Err(_) => (GlyphLookup::Full, true),
        }
    }

    /// Find a Tier B path mask, rasterizing (R14) and staging it on a miss. The slot's
    /// `left`/`top` place it relative to the request origin like a glyph (`x + left`, `y − top`).
    pub(crate) fn lookup_path(
        &mut self,
        key: MaskKey,
        src: &MaskSource,
        stats: &mut FrameStats,
    ) -> (GlyphLookup, bool) {
        if let Some(hit) = self.cpu.get(key) {
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
            self.cpu.insert_empty(key);
            return (GlyphLookup::Empty, true);
        };
        let (Ok(left), Ok(top)) = (i16::try_from(placement.0), i16::try_from(-placement.1)) else {
            return (GlyphLookup::Unsupported, true);
        };
        let (w, h) = (self.mask_buf.width, self.mask_buf.height);
        match self.cpu.insert(key, AtlasKind::Mask, w, h, left.into(), top.into(), stats) {
            Ok(slot) => {
                stage(&mut self.staging, &mut self.uploads, slot, &self.mask_buf.data);
                (GlyphLookup::Slot(slot), true)
            }
            Err(_) => (GlyphLookup::Full, true),
        }
    }

    /// SYNC (10 §5.7–§5.8): grow the textures (separate submit), then upload this frame's staged
    /// entries, then refresh the bind group keyed by both generations (M12).
    pub(crate) fn flush(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layout: &wgpu::BindGroupLayout,
        stats: &mut FrameStats,
    ) {
        self.mask.sync(device, queue, &self.cpu.mask, stats);
        self.color.sync(device, queue, &self.cpu.color, stats);
        for u in &self.uploads {
            let (tex, bpp) = match u.kind {
                AtlasKind::Mask => (&self.mask.texture, 1),
                AtlasKind::Color => (&self.color.texture, 4),
            };
            let Some(bytes) = self.staging.get(u.bytes.clone()) else { continue };
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: u32::from(u.x), y: u32::from(u.y), z: u32::from(u.layer) },
                    aspect: wgpu::TextureAspect::All,
                },
                bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(u32::from(u.w) * bpp),
                    rows_per_image: None,
                },
                wgpu::Extent3d { width: u32::from(u.w), height: u32::from(u.h), depth_or_array_layers: 1 },
            );
            stats.texture_bytes_uploaded += bytes.len() as u64;
        }
        stats.glyph_atlas_occupancy_pct = self.cpu.mask.occupancy_pct();
        stats.color_atlas_occupancy_pct = self.cpu.color.occupancy_pct();
        let key = (self.mask.generation << 32) ^ self.color.generation;
        let (mask_view, color_view, sampler) = (&self.mask.view, &self.color.view, &self.sampler);
        refresh(
            &mut self.bind_group,
            key,
            || {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("pebbles.gpu.bind.glyph"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(mask_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(color_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(sampler),
                        },
                    ],
                })
            },
            stats,
        );
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

#[cfg(test)]
#[test]
fn premultiply_rgba8_in_place_is_exact() {
    let mut px = [255, 0, 0, 128, 10, 20, 30, 255, 200, 100, 50, 0];
    premultiply_rgba8_in_place(&mut px);
    assert_eq!(px, [128, 0, 0, 128, 10, 20, 30, 255, 0, 0, 0, 0]);
}
