//! Glyph-run preparation: transform the glyph runs into physical-pixel space.

use std::ops::Range;

use kurbo::Affine;
use peniko::FontData;

use pebbles_render::paint::Glyph;

use super::num::{index_u32, to_f32};
use super::plan::{FramePlan, GlyphRun};

/// Append one glyph run (transformed to physical-pixel space) and return its arena range.
#[allow(clippy::too_many_arguments)]
pub(crate) fn glyph_run(
    plan: &mut FramePlan,
    font: &FontData,
    font_size: f32,
    font_weight: u16,
    color: [f32; 4],
    transform: Affine,
    glyph_transform: Option<Affine>,
    glyphs: &[Glyph],
) -> Range<u32> {
    let coeffs = transform.as_coeffs();
    let scale = to_f32((coeffs[0] * coeffs[0] + coeffs[1] * coeffs[1]).sqrt().max(1e-6)).unwrap_or(0.0);
    let glyphs = glyphs
        .iter()
        .map(|g| {
            let p = transform * kurbo::Point::new(f64::from(g.x), f64::from(g.y));
            Glyph { id: g.id, x: to_f32(p.x).unwrap_or(0.0), y: to_f32(p.y).unwrap_or(0.0) }
        })
        .collect();
    let start = index_u32(plan.glyph_runs.len());
    plan.glyph_runs.push(GlyphRun {
        font: font.clone(),
        font_size: font_size * scale,
        font_weight,
        color,
        glyphs,
        synthesize_italic: glyph_transform.is_some(),
    });
    start..start + 1
}
