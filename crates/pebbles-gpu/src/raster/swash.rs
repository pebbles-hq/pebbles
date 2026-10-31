//! Glyph rasterization with swash directly from the op's font bytes (DR-07, 02 §7).
//! The renderer owns no font database and never scans system fonts (00 N11).

use std::collections::HashMap;

use swash::scale::image::Image;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::Format;

/// swash renders only bitmap strikes, at their own ppem (R23: never resampled by swash, whose
/// Mitchell filter runs on straight alpha). COLR glyphs are fragments drawn by our pipeline (R29).
/// Plain outlines go through the Tier B coverage rasterizer (`outline_mask`), so glyph and path
/// edges get identical, winding-exact coverage (DR-23) — swash's own mask renderer
/// double-counts the overlapping contours variable fonts keep.
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
    /// Subpixel (LCD) coverage, RGBA8 per pixel: R, G, B = the filtered coverage of the
    /// left, middle and right subpixel in panel order, A = their maximum (`lcd_mask`).
    Lcd,
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
/// Largest atlas glyph box per side (px). Atlas glyphs are at most `MAX_ATLAS_GLYPH_PX` per em
/// (larger sizes take the outline route, 02 §7.5); a real glyph stays within a few em even with
/// swashes or stacked marks, so 4 em is generous. A malformed outline can claim tens of
/// thousands of px for a 5 px glyph — found by the S32 fuzzer — and would otherwise allocate
/// and fill a mask of hundreds of megapixels; it is skipped instead.
pub(crate) const MAX_RASTER_GLYPH_PX: u32 = 4 * crate::limits::MAX_ATLAS_GLYPH_PX;

/// The pixel box of a glyph outline of `w` × `h`, or `None` beyond [`MAX_RASTER_GLYPH_PX`].
fn glyph_box(w: f64, h: f64) -> Option<(u32, u32)> {
    let max = f64::from(MAX_RASTER_GLYPH_PX);
    if !(w <= max && h <= max) {
        return None; // NaN fails the comparison too
    }
    Some((
        crate::prepare::num::ceil_px(w, MAX_RASTER_GLYPH_PX)?,
        crate::prepare::num::ceil_px(h, MAX_RASTER_GLYPH_PX)?,
    ))
}

/// What [`GlyphRaster::decode`] left in the scratch buffers.
enum Decoded {
    /// A color strike decoded at this ppem into `scratch`.
    Strike(u16),
    /// An outline in `outline`, to be shifted right by this many px.
    Outline(f64),
}

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
    /// Faces whose decoding panicked inside the font stack (malformed tables): skipped from
    /// then on, logged once each.
    poisoned: std::collections::HashSet<(u64, u32)>,
}

/// What to rasterize.
pub(crate) struct RasterRequest<'a> {
    pub font: &'a peniko::FontData,
    pub glyph_id: u16,
    pub size_px: f32,
    pub subpx_x: u8,
    pub coords: &'a [i16],
    /// Vertical hinting (02 §7.8, `GlyphKey::hint`).
    pub hint: bool,
    /// Bitmap strikes: keep the largest strike at native size (the outline route draws it as a
    /// transformed quad, R23 step 2a).
    pub native_strike: bool,
    /// Subpixel coverage for outlines (`GlyphKey::lcd`): 0 = grayscale, 1 = RGB, 2 = BGR.
    pub lcd: u8,
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
        strikes.find_by_nearest_ppem(req.size_px.ceil().clamp(1.0, 65535.0) as u16, gid)
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
            poisoned: std::collections::HashSet::new(),
        }
    }
}

impl GlyphRaster {
    /// Rasterize one glyph. `None` when the font cannot be parsed or swash has no
    /// representation for the glyph.
    ///
    /// A malformed face can panic inside the font stack (e.g. read-fonts 0.41 indexing an empty
    /// glyf flag array, found by the S33 mutation loop): the panic is contained here, the glyph
    /// is skipped and the face is never decoded again — a bad font costs its glyphs, never the
    /// app.
    pub(crate) fn rasterize(&mut self, req: &RasterRequest<'_>) -> Option<RasterGlyph<'_>> {
        let id = (req.font.data.id(), req.font.index);
        if self.poisoned.contains(&id) {
            return None;
        }
        // Only the font-stack work (table parsing, strike decode, outline scaling) runs inside
        // the boundary; it fills our scratch buffers and returns an owned result, so the glyph
        // is decoded once and the borrowed RasterGlyph is built outside.
        let decoded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.decode(req)));
        match decoded {
            Ok(Some(Decoded::Strike(ppem))) => self.strike(ppem, req),
            Ok(Some(Decoded::Outline(dx))) if req.lcd != 0 => self.lcd_mask(dx, req.lcd == 2),
            Ok(Some(Decoded::Outline(dx))) => self.outline_mask(dx),
            Ok(None) => None,
            Err(_) => {
                self.poisoned.insert(id);
                self.fonts.remove(&id);
                crate::diag::warn_once_font_panic(id.0);
                None
            }
        }
    }

    /// Decode `req` through the font stack into `self.scratch` (a color strike) or
    /// `self.outline` (an outline shifted by the returned `dx`).
    fn decode(&mut self, req: &RasterRequest<'_>) -> Option<Decoded> {
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
        let dx = f32::from(req.subpx_x) * 0.25;
        // Source order (02 §7.2, end state): COLR glyphs never reach the rasterizer (R29 draws
        // them as fragments); (2) a color strike at its own ppem (R23); (3) the outline.
        if let Some(ppem) = strike_ppem(&font, req) {
            let mut strike = self.ctx.builder(font).size(f32::from(ppem)).build();
            // The scratch image is reused: clear it, since swash only resizes it (02 §7.2).
            self.scratch.clear();
            if Render::new(STRIKE_SOURCES).format(Format::Alpha).render_into(
                &mut strike,
                req.glyph_id,
                &mut self.scratch,
            ) {
                return Some(Decoded::Strike(ppem));
            }
        }
        let mut scaler = self
            .ctx
            .builder(font)
            .size(req.size_px)
            .hint(req.hint)
            .normalized_coords(req.coords.iter().copied())
            .build();
        self.outline.clear();
        if !scaler.scale_outline_into(req.glyph_id, &mut self.outline) {
            return None;
        }
        Some(Decoded::Outline(f64::from(dx)))
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

    /// `self.outline` (y-up, pen-relative) shifted right by `dx` px as a y-down path in
    /// `self.path`.
    fn build_path(&mut self, dx: f64) -> Option<()> {
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
        Some(())
    }

    /// Rasterize `self.outline` (y-up, pen-relative) shifted right by `dx` px into `self.mask`,
    /// placed like swash's images: `left` px right of the pen, `top` px above the baseline.
    fn outline_mask(&mut self, dx: f64) -> Option<RasterGlyph<'_>> {
        self.build_path(dx)?;
        use kurbo::Shape as _;
        let b = self.path.bounding_box();
        let (x0, y0) = (b.x0.floor(), b.y0.floor());
        let (w, h) = glyph_box(b.x1 - x0, b.y1 - y0)?;
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

    /// Subpixel (LCD) coverage of `self.outline` shifted by `dx` (FreeType / Skia practice):
    /// rasterized at 3× horizontal resolution by the same exact coverage rasterizer, filtered
    /// with FreeType's default 5-tap LCD filter (`LCD_FILTER`, against colour fringes), then
    /// packed three subpixels per pixel into `self.color` (`RasterContent::Lcd`). The box gets
    /// one px of padding on each side for the filter's spread.
    fn lcd_mask(&mut self, dx: f64, bgr: bool) -> Option<RasterGlyph<'_>> {
        self.build_path(dx)?;
        use kurbo::Shape as _;
        let b = self.path.bounding_box();
        let (x0, y0) = (b.x0.floor() - 1.0, b.y0.floor());
        let (w, h) = glyph_box(b.x1 - x0 + 1.0, b.y1 - y0)?;
        let shift = kurbo::Affine::scale_non_uniform(3.0, 1.0) * kurbo::Affine::translate((-x0, -y0));
        let clip = crate::prepare::ScissorRect { x0: 0, y0: 0, x1: 3 * w, y1: h };
        let placed =
            self.coverage.rasterize(&self.path, shift, peniko::Fill::NonZero, clip, &mut self.mask)?;
        // Re-place the (tight) 3× mask into the full 3w × h box.
        let (pw, ph) = (self.mask.width as usize, self.mask.height as usize);
        let (ox, oy) = (usize::try_from(placed.left).ok()?, usize::try_from(placed.top).ok()?);
        let (sw, wh) = (3 * w as usize, h as usize);
        let mut sub = vec![0u8; sw * wh];
        for y in 0..ph.min(wh.saturating_sub(oy)) {
            let n = pw.min(sw.saturating_sub(ox));
            sub[(oy + y) * sw + ox..(oy + y) * sw + ox + n]
                .copy_from_slice(&self.mask.data[y * pw..y * pw + n]);
        }
        lcd_pack(&sub, w as usize, wh, bgr, &mut self.color);
        let to_i32 = |v: f64| {
            i32::try_from(crate::prepare::num::to_u64_sat(v.abs())).ok().map(|m| if v < 0.0 { -m } else { m })
        };
        Some(RasterGlyph {
            left: to_i32(x0)?,
            top: -to_i32(y0)?,
            width: w,
            height: h,
            content: RasterContent::Lcd,
            strike_ppem: 0,
            data: &self.color,
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

/// FreeType's default LCD filter (`FT_LCD_FILTER_DEFAULT`), weights /256, sum 256.
pub(crate) const LCD_FILTER: [u32; 5] = [0x08, 0x4D, 0x56, 0x4D, 0x08];

/// Filter each row of the `3w × h` subpixel coverage `sub` with [`LCD_FILTER`] (zero outside)
/// and pack it as RGBA8 into `out`: R, G, B = subpixels 0, 1, 2 of each pixel (swapped for a BGR
/// panel, so R is always the red subpixel's coverage), A = their maximum.
pub(crate) fn lcd_pack(sub: &[u8], w: usize, h: usize, bgr: bool, out: &mut Vec<u8>) {
    out.clear();
    out.resize(w * h * 4, 0);
    let sw = 3 * w;
    for y in 0..h {
        let row = &sub[y * sw..(y + 1) * sw];
        // Tap t of subpixel i reads subpixel i + t − 2 (zero outside the row).
        let at = |k: usize| k.checked_sub(2).and_then(|k| row.get(k)).map_or(0, |&v| u32::from(v));
        for x in 0..w {
            let mut c = [0u8; 3];
            for (k, ck) in c.iter_mut().enumerate() {
                let i = 3 * x + k;
                let v: u32 = LCD_FILTER.iter().enumerate().map(|(t, wt)| wt * at(i + t)).sum();
                *ck = u8::try_from((v + 128) >> 8).unwrap_or(u8::MAX);
            }
            if bgr {
                c.swap(0, 2);
            }
            let o = (y * w + x) * 4;
            out[o..o + 3].copy_from_slice(&c);
            out[o + 3] = c[0].max(c[1]).max(c[2]);
        }
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
        let req = RasterRequest {
            font,
            glyph_id: id,
            size_px: 32.0,
            subpx_x: 0,
            coords,
            hint: false,
            native_strike: false,
            lcd: 0,
        };
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
            hint: false,
            native_strike: false,
            lcd: 0,
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

    #[test]
    fn oversized_glyph_box_is_refused() {
        assert_eq!(glyph_box(30.0, 40.0), Some((30, 40)));
        assert!(glyph_box(1024.0, 1024.0).is_some());
        assert_eq!(glyph_box(25_635.4, 26_015.8), None, "the fuzzer's malformed 5 px glyph");
        assert_eq!(glyph_box(f64::NAN, 10.0), None);
    }

    fn rasterize_h(raster: &mut GlyphRaster, font: &peniko::FontData) {
        let req = RasterRequest {
            font,
            glyph_id: glyph_h(font),
            size_px: 16.0,
            subpx_x: 0,
            coords: &[],
            hint: false,
            native_strike: false,
            lcd: 0,
        };
        raster.rasterize(&req);
    }

    fn image(source: Source, px: [u8; 4]) -> Image {
        use swash::scale::image::Content;
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

    /// Over the pangram's glyphs: `Σ (c[x][y+1] − c[x][y])²` (vertical gradient energy) per
    /// unit of ink `Σ c`. Normalized because hinting snaps x-height/cap height, changing the
    /// number of rows: the raw sum rewards blur spread over more rows (an 11-px 'e' has 8
    /// blurred rows unhinted, 6 crisp ones hinted).
    fn row_sharpness(raster: &mut GlyphRaster, font: &peniko::FontData, ppem: f32, hint: bool) -> f64 {
        let f = swash::FontRef::from_index(font.data.data(), 0).unwrap();
        let mut total = 0.0;
        let mut ink = 0.0;
        for ch in
            "The quick brown fox jumps over the lazy dog 0123456789".chars().filter(|c| !c.is_whitespace())
        {
            let req = RasterRequest {
                font,
                glyph_id: f.charmap().map(ch),
                size_px: ppem,
                subpx_x: 0,
                coords: &[],
                hint,
                native_strike: false,
                lcd: 0,
            };
            let Some(g) = raster.rasterize(&req) else { continue };
            let w = g.width as usize;
            // Zero rows above and below: the mask's own top/bottom edges are transitions too
            // (a crisp hinted edge sits exactly there).
            let zero = vec![0u8; w];
            let rows: Vec<&[u8]> = std::iter::once(zero.as_slice())
                .chain(g.data.chunks(w.max(1)))
                .chain(std::iter::once(zero.as_slice()))
                .collect();
            total += rows
                .windows(2)
                .flat_map(|p| p[0].iter().zip(p[1]).map(|(a, b)| (f64::from(*b) - f64::from(*a)).powi(2)))
                .sum::<f64>();
            ink += g.data.iter().map(|&c| f64::from(c)).sum::<f64>();
        }
        total / ink
    }

    /// R27 AC3 / step 4: vertical hinting sharpens horizontal features at 1× for 11–16 px
    /// (`auto` = hinted there) and is never worse; the table is printed for the evidence.
    #[test]
    #[allow(clippy::print_stderr)] // the measurement table is the evidence (R27 step 4)
    fn hinting_sharpens_rows() {
        let font = inter();
        let mut raster = GlyphRaster::default();
        for scale in [1.0f32, 1.25] {
            for size in [10.0f32, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 24.0] {
                let ppem = size * scale;
                let off = row_sharpness(&mut raster, &font, ppem, false);
                let auto_hint = crate::prepare::glyphs::hint_policy(f64::from(scale), ppem);
                let auto = row_sharpness(&mut raster, &font, ppem, auto_hint);
                eprintln!("scale {scale} size {size}: auto {auto:.2} off {off:.2} ratio {:.3}", auto / off);
                if scale == 1.0 && (11.0..=16.0).contains(&size) {
                    assert!(auto >= off, "size {size}: auto {auto} < off {off}");
                }
            }
        }
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
                hint: false,
                native_strike: false,
                lcd: 0,
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

    fn lcd_req(font: &peniko::FontData, lcd: u8) -> RasterRequest<'_> {
        RasterRequest {
            font,
            glyph_id: glyph_h(font),
            size_px: 13.0,
            subpx_x: 0,
            coords: &[],
            hint: true,
            native_strike: false,
            lcd,
        }
    }

    #[test]
    fn lcd_filter_is_normalized() {
        assert_eq!(LCD_FILTER.iter().sum::<u32>(), 256);
        // Full coverage stays full inside; the filter spreads ≤ 2 subpixels past an edge.
        let mut out = Vec::new();
        let mut sub = vec![0u8; 9];
        sub[3..6].fill(255);
        lcd_pack(&sub, 3, 1, false, &mut out);
        // A one-px stem: (8+77+86)/256, (77+86+77)/256, (86+77+8)/256 of 255.
        assert_eq!(&out[4..8], &[170, 239, 170, 239], "middle px: filtered stem");
        assert_eq!(&out[0..3], &[0, 8, 85], "left px: the filter's spread");
        assert_eq!(&out[8..11], &[85, 8, 0], "right px");
        let wide = vec![255u8; 15];
        lcd_pack(&wide, 5, 1, false, &mut out);
        assert_eq!(&out[8..12], &[255, 255, 255, 255]);
    }

    /// The atlas holds three distinct coverage channels for an LCD glyph, with fringes, and
    /// a BGR panel swaps R and B.
    #[test]
    fn lcd_glyph_has_three_channels_and_bgr_swaps() {
        let font = inter();
        let mut raster = GlyphRaster::default();
        let rgb = raster.rasterize(&lcd_req(&font, 1)).map(|g| {
            assert_eq!(g.content, RasterContent::Lcd);
            assert_eq!(g.data.len(), g.width as usize * g.height as usize * 4);
            (g.data.to_vec(), g.left, g.width)
        });
        let (rgb, left, width) = rgb.unwrap();
        let px: Vec<&[u8]> = rgb.chunks(4).collect();
        assert!(px.iter().any(|p| p[0] != p[2]), "no colour fringe: channels identical");
        assert!(px.iter().any(|p| p[1] >= 230), "solid stem");
        assert!(px.iter().all(|p| p[3] == p[0].max(p[1]).max(p[2])));
        let gray = raster.rasterize(&lcd_req(&font, 0)).map(|g| (g.left, g.width)).unwrap();
        assert_eq!(left, gray.0 - 1, "one px of filter padding on the left");
        assert!(width > gray.1);
        let bgr = raster.rasterize(&lcd_req(&font, 2)).unwrap().data.to_vec();
        for (a, b) in rgb.chunks(4).zip(bgr.chunks(4)) {
            assert_eq!([a[0], a[1], a[2]], [b[2], b[1], b[0]]);
        }
    }
}
