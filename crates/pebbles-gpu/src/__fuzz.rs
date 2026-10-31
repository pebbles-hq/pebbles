//! Fuzzing entry points (11 S31). NOT public API — no semver guarantees.

use crate::raster::swash::{GlyphRaster, RasterRequest};

/// Rasterize `glyphs` of face `index` at `px` through the SAME CPU raster function the glyph atlas uses (C09);
/// return the total coverage bytes produced. Must never panic.
pub fn raster_font_bytes(font: &[u8], index: u32, glyphs: &[u16], px: f32) -> usize {
    let px = if px.is_finite() { px.clamp(1.0, 512.0) } else { 16.0 };
    let font = peniko::FontData::new(peniko::Blob::new(std::sync::Arc::new(font.to_vec())), index);
    let mut raster = GlyphRaster::default();
    let mut total = 0;
    for &glyph_id in glyphs {
        let req = RasterRequest {
            font: &font,
            glyph_id,
            size_px: px,
            subpx_x: 0,
            coords: &[],
            hint: false,
            native_strike: false,
            lcd: u8::from(glyph_id % 2 == 1),
        };
        if let Some(g) = raster.rasterize(&req) {
            total += g.data.len();
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_font_rasterizes() {
        let inter = pebbles_render::fonts::builtin_fonts()[0].1;
        assert!(raster_font_bytes(inter, 0, &(1..40).collect::<Vec<u16>>(), 16.0) > 0);
    }

    #[test]
    fn garbage_returns_zero() {
        assert_eq!(raster_font_bytes(&[], 0, &[1, 2, 3], 16.0), 0);
        let noise: Vec<u8> = (0u8..16).map(|i| i.wrapping_mul(97).wrapping_add(13)).collect();
        assert_eq!(raster_font_bytes(&noise, 3, &[0, 1, 2], f32::NAN), 0);
    }
}
