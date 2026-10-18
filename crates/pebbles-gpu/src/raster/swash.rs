//! Glyph rasterization with swash directly from the op's font bytes (DR-07, 02 §7).
//! The renderer owns no font database and never scans system fonts (00 N11).

use std::collections::HashMap;

use swash::scale::image::{Content, Image};
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Format, Vector};

/// Color raster sources in priority order: COLR at the requested size, then bitmap strikes at
/// their own ppem (R23: never resampled by swash, whose Mitchell filter runs on straight alpha).
/// Plain outlines are not rendered by swash: they go through the Tier B coverage rasterizer
/// (`outline_mask`), so glyph and path edges get identical, winding-exact coverage (DR-23) —
/// swash's own mask renderer double-counts the overlapping contours variable fonts keep.
const COLR_SOURCES: &[Source] = &[Source::ColorOutline(0)];
/// A strike decoded at exactly the scaler's size (= the strike's ppem): no swash resize.
const STRIKE_SOURCES: &[Source] = &[Source::ColorBitmap(StrikeWith::ExactSize)];

/// The bytes to upload for a swash image: premultiplied RGBA into `scratch` for
/// `Source::ColorBitmap` (PNG strikes are straight alpha), else the image data unchanged (COLR
/// output and masks are already premultiplied / coverage; 10 §5.8).
pub(crate) fn color_upload_bytes<'a>(image: &'a Image, scratch: &'a mut Vec<u8>) -> &'a [u8] {
    if matches!(image.source, Source::ColorBitmap(_)) {
        scratch.clear();
        scratch.extend_from_slice(&image.data);
        crate::execute::resources::glyph_atlas::premultiply_rgba8_in_place(scratch);
        scratch
    } else {
        &image.data
    }
}

/// Whether a raster is coverage or premultiplied-or-straight RGBA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RasterContent {
    /// One byte of coverage per pixel.
    Mask,
    /// RGBA8 per pixel.
    Color,
    /// A subpixel mask (never requested).
    Subpixel,
}

/// One rasterized glyph, borrowed from the rasterizer's scratch image.
pub(crate) struct RasterGlyph<'a> {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub content: RasterContent,
    /// A bitmap strike kept at its native size: draw it `font_size / strike_ppem` × (R23).
    /// 0 = draw at bitmap size. Data is always premultiplied (or coverage).
    pub strike_ppem: u16,
    pub data: &'a [u8],
}

/// swash scaler-cache entries (§5.11): the maximum swash allows; the default 8 thrashes.
pub(crate) const SCALE_CONTEXT_MAX_ENTRIES: usize = 64;

/// Frames between sweeps of dead font entries (§5.11).
const FONT_SWEEP_EVERY_FRAMES: u64 = 256;

/// Per font face: liveness of the bytes and the swash lookup data, built once.
struct RasterFont {
    blob: peniko::WeakBlob<u8>,
    offset: u32,
    key: swash::CacheKey,
}

/// The glyph rasterizer: one swash `ScaleContext` plus one reused scratch image.
pub(crate) struct GlyphRaster {
    frame: u64,
    ctx: ScaleContext,
    fonts: HashMap<(u64, u32), RasterFont>,
    scratch: Image,
    outline: swash::scale::outline::Outline,
    path: kurbo::BezPath,
    coverage: super::coverage::CoverageRasterizer,
    mask: super::coverage::MaskBuf,
    /// Premultiplied strike bytes, then their box-downscaled copy (R23).
    color: Vec<u8>,
    down: Vec<u8>,
}

/// What to rasterize.
pub(crate) struct RasterRequest<'a> {
    pub font: &'a peniko::FontData,
    pub glyph_id: u16,
    pub size_px: f32,
    pub subpx_x: u8,
    pub coords: &'a [i16],
    /// Bitmap strikes: keep the largest strike at native size (the outline route draws it as a
    /// transformed quad, R23 step 2a).
    pub native_strike: bool,
}

/// The color strike to decode for `req`: the largest for the outline route, else the nearest
/// to the requested ppem. `None` when the glyph has no color bitmap.
fn strike_ppem(font: &swash::FontRef<'_>, req: &RasterRequest<'_>) -> Option<u16> {
    let strikes = font.color_strikes();
    let gid = req.glyph_id;
    let strike = if req.native_strike {
        strikes.find_by_largest_ppem(gid)
    } else {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // ≤ MAX_ATLAS_GLYPH_PX
        strikes.find_by_nearest_ppem(req.size_px.round().clamp(1.0, 65535.0) as u16, gid)
    }?;
    (strike.ppem() > 0).then(|| strike.ppem())
}

impl Default for GlyphRaster {
    fn default() -> Self {
        Self {
            frame: 0,
            ctx: ScaleContext::with_max_entries(SCALE_CONTEXT_MAX_ENTRIES),
            fonts: HashMap::new(),
            scratch: Image::new(),
            outline: swash::scale::outline::Outline::new(),
            path: kurbo::BezPath::new(),
            coverage: super::coverage::CoverageRasterizer::default(),
            mask: super::coverage::MaskBuf::default(),
            color: Vec::new(),
            down: Vec::new(),
        }
    }
}

impl GlyphRaster {
    /// Rasterize one glyph. `None` when the font cannot be parsed or swash has no
    /// representation for the glyph.
    pub(crate) fn rasterize(&mut self, req: &RasterRequest<'_>) -> Option<RasterGlyph<'_>> {
        let bytes: &[u8] = req.font.data.data();
        let id = (req.font.data.id(), req.font.index);
        let entry = match self.fonts.entry(id) {
            std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::hash_map::Entry::Vacant(v) => {
                let font = swash::FontRef::from_index(bytes, req.font.index as usize)?;
                v.insert(RasterFont { blob: req.font.data.downgrade(), offset: font.offset, key: font.key })
            }
        };
        let font = swash::FontRef { data: bytes, offset: entry.offset, key: entry.key };
        let mut scaler = self
            .ctx
            .builder(font)
            .size(req.size_px)
            .hint(false)
            .normalized_coords(req.coords.iter().copied())
            .build();
        let dx = f32::from(req.subpx_x) * 0.25;
        let mut render = Render::new(COLR_SOURCES);
        render.format(Format::Alpha).offset(Vector::new(dx, 0.0));
        // The scratch image is reused: clear it, since swash only resizes it and zeno
        // writes only covered cells (02 §7.2).
        self.scratch.clear();
        if !render.render_into(&mut scaler, req.glyph_id, &mut self.scratch) {
            if let Some(ppem) = strike_ppem(&font, req) {
                let mut strike = self.ctx.builder(font).size(f32::from(ppem)).build();
                self.scratch.clear();
                if Render::new(STRIKE_SOURCES).format(Format::Alpha).render_into(
                    &mut strike,
                    req.glyph_id,
                    &mut self.scratch,
                ) {
                    return self.strike(ppem, req);
                }
                scaler = self
                    .ctx
                    .builder(font)
                    .size(req.size_px)
                    .hint(false)
                    .normalized_coords(req.coords.iter().copied())
                    .build();
            }
            self.outline.clear();
            if !scaler.scale_outline_into(req.glyph_id, &mut self.outline) {
                return None;
            }
            return self.outline_mask(f64::from(dx));
        }
        let img = &self.scratch;
        Some(RasterGlyph {
            left: img.placement.left,
            top: img.placement.top,
            width: img.placement.width,
            height: img.placement.height,
            content: match img.content {
                Content::Mask => RasterContent::Mask,
                Content::Color => RasterContent::Color,
                Content::SubpixelMask => RasterContent::Subpixel,
            },
            strike_ppem: 0,
            data: &img.data,
        })
    }

    /// The strike in `self.scratch` (decoded at `ppem`) premultiplied; box-downscaled to the
    /// requested size when larger, else kept native with `strike_ppem` set (§7.2, R23 1b).
    fn strike(&mut self, ppem: u16, req: &RasterRequest<'_>) -> Option<RasterGlyph<'_>> {
        let p = self.scratch.placement;
        let premul = color_upload_bytes(&self.scratch, &mut self.color);
        let s = req.size_px / f32::from(ppem);
        if req.native_strike || s >= 1.0 {
            return Some(RasterGlyph {
                left: p.left,
                top: p.top,
                width: p.width,
                height: p.height,
                content: RasterContent::Color,
                strike_ppem: ppem,
                data: premul,
            });
        }
        let (width, height) = super::bitmap::downscale_premul(premul, p.width, p.height, s, &mut self.down);
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // bearings ≤ 2^16
        let round = |v: i32| (v as f32 * s).round() as i32;
        Some(RasterGlyph {
            left: round(p.left),
            top: round(p.top),
            width,
            height,
            content: RasterContent::Color,
            strike_ppem: 0,
            data: &self.down,
        })
    }

    /// Rasterize `self.outline` (y-up, pen-relative) shifted right by `dx` px into `self.mask`,
    /// placed like swash's images: `left` px right of the pen, `top` px above the baseline.
    fn outline_mask(&mut self, dx: f64) -> Option<RasterGlyph<'_>> {
        use swash::zeno::Verb;
        let pts = self.outline.points();
        let p = |i: usize| pts.get(i).map(|q| kurbo::Point::new(dx + f64::from(q.x), -f64::from(q.y)));
        self.path.truncate(0);
        let mut i = 0;
        for verb in self.outline.verbs() {
            match verb {
                Verb::MoveTo => self.path.move_to(p(i)?),
                Verb::LineTo => self.path.line_to(p(i)?),
                Verb::QuadTo => self.path.quad_to(p(i)?, p(i + 1)?),
                Verb::CurveTo => self.path.curve_to(p(i)?, p(i + 1)?, p(i + 2)?),
                Verb::Close => self.path.close_path(),
            }
            i += match verb {
                Verb::MoveTo | Verb::LineTo => 1,
                Verb::QuadTo => 2,
                Verb::CurveTo => 3,
                Verb::Close => 0,
            };
        }
        use kurbo::Shape as _;
        let b = self.path.bounding_box();
        let (x0, y0) = (b.x0.floor(), b.y0.floor());
        let w = crate::prepare::num::ceil_px(b.x1 - x0, u32::from(u16::MAX))?;
        let h = crate::prepare::num::ceil_px(b.y1 - y0, u32::from(u16::MAX))?;
        let shift = kurbo::Affine::translate((-x0, -y0));
        let clip = crate::prepare::ScissorRect { x0: 0, y0: 0, x1: w, y1: h };
        let placed =
            self.coverage.rasterize(&self.path, shift, peniko::Fill::NonZero, clip, &mut self.mask)?;
        let left = i32::try_from(crate::prepare::num::to_u64_sat(x0.abs())).ok()?;
        let top = i32::try_from(crate::prepare::num::to_u64_sat(y0.abs())).ok()?;
        let left = if x0 < 0.0 { -left } else { left } + placed.left;
        let top = -(if y0 < 0.0 { -top } else { top } + placed.top);
        Some(RasterGlyph {
            left,
            top,
            width: self.mask.width,
            height: self.mask.height,
            content: RasterContent::Mask,
            strike_ppem: 0,
            data: &self.mask.data,
        })
    }

    /// End of frame: every `FONT_SWEEP_EVERY_FRAMES` frames, drop fonts whose bytes are gone.
    pub(crate) fn end_frame(&mut self) {
        self.frame += 1;
        if self.frame.is_multiple_of(FONT_SWEEP_EVERY_FRAMES) {
            self.sweep();
        }
    }

    /// Drop fonts whose bytes are gone (the census reads the count, 10 §9.2 `raster_fonts`).
    fn sweep(&mut self) {
        self.fonts.retain(|_, e| e.blob.upgrade().is_some());
    }

    /// The swash cache key used for `font` (tests).
    #[cfg(test)]
    fn key_of(&self, font: &peniko::FontData) -> Option<swash::CacheKey> {
        self.fonts.get(&(font.data.id(), font.index)).map(|e| e.key)
    }

    /// Faces currently known (for tests and the census, `raster_fonts`).
    #[cfg(any(test, debug_assertions))]
    pub(crate) fn font_count(&self) -> usize {
        self.fonts.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inter() -> peniko::FontData {
        crate::test_support::bundled_font()
    }

    fn glyph_h(font: &peniko::FontData) -> u16 {
        let f = swash::FontRef::from_index(font.data.data(), font.index as usize).unwrap();
        f.charmap().map('H')
    }

    fn coverage_sum(raster: &mut GlyphRaster, font: &peniko::FontData, coords: &[i16]) -> u64 {
        let id = glyph_h(font);
        let req =
            RasterRequest { font, glyph_id: id, size_px: 32.0, subpx_x: 0, coords, native_strike: false };
        raster.rasterize(&req).unwrap().data.iter().map(|&b| u64::from(b)).sum()
    }

    #[test]
    fn rasterize_h_has_coverage() {
        let font = inter();
        let mut raster = GlyphRaster::default();
        let req = RasterRequest {
            font: &font,
            glyph_id: glyph_h(&font),
            size_px: 16.0,
            subpx_x: 0,
            coords: &[],
            native_strike: false,
        };
        let g = raster.rasterize(&req).unwrap();
        assert_eq!(g.content, RasterContent::Mask);
        assert!((8..=14).contains(&g.width), "width {}", g.width);
        assert!((10..=14).contains(&g.height), "height {}", g.height);
        assert!(g.data.contains(&255));
    }

    #[test]
    fn weight_axis_changes_coverage() {
        let font = inter();
        let f = swash::FontRef::from_index(font.data.data(), font.index as usize).unwrap();
        let coords_for = |w: f32| -> Vec<i16> {
            f.variations().normalized_coords([(swash::tag_from_str_lossy("wght"), w)]).collect()
        };
        let mut raster = GlyphRaster::default();
        let regular = coverage_sum(&mut raster, &font, &coords_for(400.0));
        let bold = coverage_sum(&mut raster, &font, &coords_for(700.0));
        assert!(bold > regular, "bold {bold} <= regular {regular}");
    }

    fn rasterize_h(raster: &mut GlyphRaster, font: &peniko::FontData) {
        let req = RasterRequest {
            font,
            glyph_id: glyph_h(font),
            size_px: 16.0,
            subpx_x: 0,
            coords: &[],
            native_strike: false,
        };
        raster.rasterize(&req);
    }

    fn image(source: Source, px: [u8; 4]) -> Image {
        let mut img = Image::new();
        img.source = source;
        img.content = Content::Color;
        img.placement = zeno::Placement { left: 0, top: 1, width: 1, height: 1 };
        img.data = px.to_vec();
        img
    }

    #[test]
    fn color_bitmap_is_premultiplied() {
        let img = image(Source::ColorBitmap(StrikeWith::ExactSize), [200, 100, 50, 128]);
        let mut scratch = Vec::new();
        assert_eq!(color_upload_bytes(&img, &mut scratch), &[100, 50, 25, 128]);
    }

    #[test]
    fn colr_output_is_uploaded_unchanged() {
        let img = image(Source::ColorOutline(0), [100, 50, 25, 128]);
        let mut scratch = Vec::new();
        assert_eq!(color_upload_bytes(&img, &mut scratch), &[100, 50, 25, 128]);
    }

    #[test]
    fn opaque_bitmap_is_unchanged() {
        let img = image(Source::ColorBitmap(StrikeWith::ExactSize), [200, 100, 50, 255]);
        let mut scratch = Vec::new();
        assert_eq!(color_upload_bytes(&img, &mut scratch), &[200, 100, 50, 255]);
    }

    #[test]
    fn font_ref_key_is_reused() {
        let font = inter();
        let mut raster = GlyphRaster::default();
        rasterize_h(&mut raster, &font);
        let first = raster.key_of(&font).unwrap();
        rasterize_h(&mut raster, &font.clone());
        assert_eq!(raster.key_of(&font), Some(first));
        assert_eq!(raster.font_count(), 1);
    }

    #[test]
    fn font_ref_cache_drops_dead_blobs() {
        let mut raster = GlyphRaster::default();
        {
            // A fresh blob (its own id), dropped at the end of the block.
            let font = inter();
            let req = RasterRequest {
                font: &font,
                glyph_id: glyph_h(&font),
                size_px: 16.0,
                subpx_x: 0,
                coords: &[],
                native_strike: false,
            };
            raster.rasterize(&req);
            raster.sweep();
            assert_eq!(raster.font_count(), 1);
        }
        for _ in 0..FONT_SWEEP_EVERY_FRAMES - 1 {
            raster.end_frame();
        }
        assert_eq!(raster.font_count(), 1, "swept only every 256 frames");
        raster.end_frame();
        assert_eq!(raster.font_count(), 0);
    }
}
