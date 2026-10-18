//! Glyph rasterization with swash directly from the op's font bytes (DR-07, 02 §7).
//! The renderer owns no font database and never scans system fonts (00 N11).

use std::collections::HashMap;

use swash::scale::image::{Content, Image};
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Format, Vector};

/// Color raster sources in priority order: COLR, then bitmap strikes. Plain outlines are not
/// rendered by swash: they go through the Tier B coverage rasterizer (`outline_mask`), so glyph
/// and path edges get identical, winding-exact coverage (DR-23) — swash's own mask renderer
/// double-counts the overlapping contours variable fonts keep.
const COLOR_SOURCES: &[Source] = &[Source::ColorOutline(0), Source::ColorBitmap(StrikeWith::BestFit)];

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
    /// False only for `Source::ColorBitmap` (PNG strikes are straight alpha).
    pub premultiplied: bool,
    pub data: &'a [u8],
}

/// Per font face: liveness of the bytes and the swash lookup data, built once.
struct FontEntry {
    blob: peniko::WeakBlob<u8>,
    offset: u32,
    key: swash::CacheKey,
}

/// The glyph rasterizer: one swash `ScaleContext` plus one reused scratch image.
#[derive(Default)]
pub(crate) struct GlyphRaster {
    ctx: ScaleContext,
    fonts: HashMap<(u64, u32), FontEntry>,
    scratch: Image,
    outline: swash::scale::outline::Outline,
    path: kurbo::BezPath,
    coverage: super::coverage::CoverageRasterizer,
    mask: super::coverage::MaskBuf,
}

/// What to rasterize.
pub(crate) struct RasterRequest<'a> {
    pub font: &'a peniko::FontData,
    pub glyph_id: u16,
    pub size_px: f32,
    pub subpx_x: u8,
    pub coords: &'a [i16],
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
                v.insert(FontEntry { blob: req.font.data.downgrade(), offset: font.offset, key: font.key })
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
        let mut render = Render::new(COLOR_SOURCES);
        render.format(Format::Alpha).offset(Vector::new(dx, 0.0));
        // The scratch image is reused: clear it, since swash only resizes it and zeno
        // writes only covered cells (02 §7.2).
        self.scratch.clear();
        if !render.render_into(&mut scaler, req.glyph_id, &mut self.scratch) {
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
            premultiplied: !matches!(img.source, Source::ColorBitmap(_)),
            data: &img.data,
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
            premultiplied: true,
            data: &self.mask.data,
        })
    }

    /// Drop fonts whose bytes are gone (end of frame).
    pub(crate) fn end_frame(&mut self) {
        self.fonts.retain(|_, e| e.blob.upgrade().is_some());
    }

    /// Faces currently known (for tests and the census).
    #[cfg(test)]
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
        let req = RasterRequest { font, glyph_id: id, size_px: 32.0, subpx_x: 0, coords };
        raster.rasterize(&req).unwrap().data.iter().map(|&b| u64::from(b)).sum()
    }

    #[test]
    fn rasterize_h_has_coverage() {
        let font = inter();
        let mut raster = GlyphRaster::default();
        let req =
            RasterRequest { font: &font, glyph_id: glyph_h(&font), size_px: 16.0, subpx_x: 0, coords: &[] };
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
    fn dead_fonts_are_dropped_at_end_of_frame() {
        let mut raster = GlyphRaster::default();
        {
            let font = inter();
            let font = peniko::FontData::new(peniko::Blob::from(font.data.data().to_vec()), 0);
            let req = RasterRequest {
                font: &font,
                glyph_id: glyph_h(&font),
                size_px: 16.0,
                subpx_x: 0,
                coords: &[],
            };
            raster.rasterize(&req);
            raster.end_frame();
            assert_eq!(raster.font_count(), 1);
        }
        raster.end_frame();
        assert_eq!(raster.font_count(), 0);
    }
}
