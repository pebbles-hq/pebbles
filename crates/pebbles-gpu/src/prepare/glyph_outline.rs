//! The glyph outline route (02 §7.5–§7.6, R20): rotated, skewed, non-uniformly scaled,
//! mirrored, `glyph_transform`ed or huge runs draw each glyph's outline as a path fill, routed
//! like any path (R16: a Tier B mask, or Tier C when huge) — crisp, never an upscaled bitmap.

use kurbo::{Affine, BezPath};
use pebbles_render::paint::Glyph;
use pebbles_render::paint::display_list::Geometry;
use swash::scale::ScaleContext;
use swash::zeno::{Point as ZenoPoint, Verb};

use super::Walk;
use super::path_strategy::{MASK_MAX_AREA, mask_area};
use super::plan::DrawCmd;
use crate::limits::MAX_ATLAS_GLYPH_PX;

/// How a glyph run is drawn (§7.5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum GlyphRoute {
    /// Atlas masks at device ppem `font_size · scale`.
    Atlas { scale: f64 },
    /// One path fill per glyph outline.
    Outline,
    /// Nothing to draw (R22 counts it).
    Skip,
}

/// §7.5 classification of a run under `m` (the brush rule is the caller's).
pub(crate) fn classify_run(m: Affine, glyph_transform: Option<Affine>, font_size: f32) -> GlyphRoute {
    if !font_size.is_finite() || font_size <= 0.0 || !m.is_finite() {
        return GlyphRoute::Skip;
    }
    if glyph_transform.is_some_and(|g| g != Affine::IDENTITY) {
        return GlyphRoute::Outline;
    }
    let [a, b, c, d, ..] = m.as_coeffs();
    let s = a.abs().max(d.abs());
    let uniform =
        b.abs() <= 1e-6 * s && c.abs() <= 1e-6 * s && a > 0.0 && d > 0.0 && (a - d).abs() <= 1e-6 * a;
    if !uniform {
        return GlyphRoute::Outline;
    }
    if f64::from(font_size) * a > f64::from(MAX_ATLAS_GLYPH_PX) {
        return GlyphRoute::Outline;
    }
    GlyphRoute::Atlas { scale: a }
}

/// swash outlines for the outline route (one reused scale context).
#[derive(Default)]
pub(crate) struct OutlineSource {
    ctx: ScaleContext,
}

impl OutlineSource {
    /// Glyph `id`'s outline at `size` px into `out`, y-up (font convention). `false` if the font
    /// cannot be parsed or the glyph has no outline (bitmap/COLR-only).
    pub(crate) fn outline(
        &mut self,
        font: &peniko::FontData,
        id: u16,
        size: f32,
        coords: &[i16],
        out: &mut BezPath,
    ) -> bool {
        out.truncate(0);
        let Some(face) = swash::FontRef::from_index(font.data.data(), font.index as usize) else {
            return false;
        };
        let mut scaler =
            self.ctx.builder(face).size(size).hint(false).normalized_coords(coords.iter()).build();
        if !scaler.has_outlines() {
            return false;
        }
        let Some(outline) = scaler.scale_outline(id) else { return false };
        let p = |q: &ZenoPoint| kurbo::Point::new(f64::from(q.x), f64::from(q.y));
        let mut pts = outline.points().iter();
        for verb in outline.verbs() {
            let ok = match verb {
                Verb::MoveTo => pts.next().map(|a| out.move_to(p(a))),
                Verb::LineTo => pts.next().map(|a| out.line_to(p(a))),
                Verb::QuadTo => pts.next().zip(pts.next()).map(|(a, b)| out.quad_to(p(a), p(b))),
                Verb::CurveTo => {
                    let (a, b, c) = (pts.next(), pts.next(), pts.next());
                    a.zip(b).zip(c).map(|((a, b), c)| out.curve_to(p(a), p(b), p(c)))
                }
                Verb::Close => {
                    out.close_path();
                    Some(())
                }
            };
            if ok.is_none() {
                return false;
            }
        }
        true
    }
}

/// `FLIP_Y`: font outlines are y-up, glyph space is y-down (§7.6).
const FLIP_Y: Affine = Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, 0.0]);

impl Walk<'_> {
    /// Draw a run on the outline route: per glyph, its outline filled (non-zero) under
    /// `m · translate(pen) · G · FLIP_Y` (§7.6), keeping the run's place in painter's order.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_glyph_outlines(
        &mut self,
        font: &peniko::FontData,
        font_size: f32,
        coords: &[i16],
        premul: [f32; 4],
        m: Affine,
        glyph_transform: Option<Affine>,
        glyphs: &[Glyph],
        idx: u32,
    ) {
        let g = glyph_transform.unwrap_or(Affine::IDENTITY);
        let mut path = BezPath::new();
        let scissor = self.clips.current().scissor;
        let linear = Affine::new({
            let [a, b, c, d, ..] = m.as_coeffs();
            [a, b, c, d, 0.0, 0.0]
        }) * g;
        for glyph in glyphs {
            let Ok(id) = u16::try_from(glyph.id) else {
                self.fell_back(super::coverage::Feature::GlyphsDegenerate);
                self.stats.record(crate::stats::Unsupported::GlyphIdRange);
                continue;
            };
            let pen = m * kurbo::Point::new(f64::from(glyph.x), f64::from(glyph.y));
            if !(pen.x.is_finite() && pen.y.is_finite()) {
                self.stats.non_finite_skipped += 1;
                self.stats.record(crate::stats::Unsupported::NonFinite);
                continue;
            }
            // R25: cull before extracting the outline.
            if super::glyphs::glyph_culled(pen, f64::from(font_size), linear, scissor) {
                self.plan.stats.culled_glyphs += 1;
                continue;
            }
            if !self.outlines.outline(font, id, font_size, coords, &mut path) {
                // Bitmap-only / COLR glyphs on this route arrive with R23 / R29.
                self.fell_back(super::coverage::Feature::GlyphsColorRotated);
                self.stats.record(crate::stats::Unsupported::ColorGlyphTransform);
                continue;
            }
            let t = m * Affine::translate((f64::from(glyph.x), f64::from(glyph.y))) * g * FLIP_Y;
            let shape = Geometry::Path(std::mem::take(&mut path));
            let masked = mask_area(&shape, t, None) <= MASK_MAX_AREA
                && self.draw_mask(&shape, t, peniko::Fill::NonZero, None, premul, idx);
            if !masked && let Geometry::Path(p) = &shape {
                // Tier C: tessellate the outline under `t` (huge glyphs).
                let els = p.elements().iter().copied();
                if let Some(indices) =
                    self.tess.fill(self.plan, self.stats, els, t, peniko::Fill::NonZero, premul)
                {
                    self.stats.tier_c_draws += 1;
                    self.push(DrawCmd::Solid { indices, clip: self.clips.current() }, idx);
                }
                self.plan.stats.tess_calls += 1;
            }
            if let Geometry::Path(p) = shape {
                path = p;
            }
        }
    }
}

#[cfg(test)]
#[path = "glyph_outline_tests.rs"]
mod tests;
