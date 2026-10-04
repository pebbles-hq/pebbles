//! Glyph-run preparation: transform the glyph runs into physical-pixel space.

use kurbo::Affine;
use peniko::{Brush, FontData};

use pebbles_render::paint::Glyph;

use super::Tess;
use super::color::premul;
use super::plan::GlyphRun;

impl Tess {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn glyph_run(
        &mut self,
        font: &FontData,
        font_size: f32,
        font_weight: u16,
        brush: &Brush,
        transform: Affine,
        glyph_transform: Option<Affine>,
        glyphs: &[Glyph],
    ) {
        let color = match brush {
            Brush::Solid(c) => premul(*c),
            // Gradient/image text is Phase 4; skip the run rather than tint it wrong.
            _ => return,
        };
        let coeffs = transform.as_coeffs();
        let scale = (coeffs[0] * coeffs[0] + coeffs[1] * coeffs[1]).sqrt().max(1e-6) as f32;
        let glyphs = glyphs
            .iter()
            .map(|g| {
                let p = transform * kurbo::Point::new(f64::from(g.x), f64::from(g.y));
                Glyph { id: g.id, x: p.x as f32, y: p.y as f32 }
            })
            .collect();
        self.glyphs.push(GlyphRun {
            font: font.clone(),
            font_size: font_size * scale,
            font_weight,
            color,
            glyphs,
            scissor: self.scissor,
            synthesize_italic: glyph_transform.is_some(),
        });
    }
}
