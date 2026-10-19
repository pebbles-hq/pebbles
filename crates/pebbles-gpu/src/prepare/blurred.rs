//! Blurred coverage masks (02 R42): `BlurredFill` (any shape) and `BlurredGlyphs` (a run's
//! union) become one Tier B mask request whose key carries the device σ and style; the atlas
//! blurs on a miss (`raster/blur.rs`). Too-large padded masks are skipped and counted until L61.

use kurbo::{Affine, BezPath, Shape as _};
use pebbles_render::paint::Glyph;
use pebbles_render::paint::display_list::{BlurStyle, Geometry};

use super::Walk;
use super::num;
use super::path_strategy::MASK_MAX_AREA;
use super::plan::{DrawCmd, GlyphRequest};
use crate::stats::Unsupported;

/// `FLIP_Y`: font outlines are y-up, glyph space is y-down.
const FLIP_Y: Affine = Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, 0.0]);

impl Walk<'_> {
    /// One blurred mask of `shape` under `xf` (σ in local units).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn draw_blurred(
        &mut self,
        shape: &Geometry,
        xf: Affine,
        rule: peniko::Fill,
        premul: [f32; 4],
        sigma: f32,
        style: BlurStyle,
        idx: u32,
    ) {
        let (smax, _) = super::stroke::singular_values(&xf);
        let sigma_dev = f64::from(sigma.max(0.0)) * smax;
        let r = (3.0 * sigma_dev).ceil();
        let b = xf.transform_rect_bbox(shape.to_path(0.1).bounding_box()).inflate(r + 1.0, r + 1.0);
        if !b.is_finite() || b.area() > MASK_MAX_AREA as f64 {
            self.stats.record(Unsupported::BlurMaskTooLarge);
            return;
        }
        let Some((mut key, mut source, origin, _)) = super::path_masks::build(shape, xf, rule, None) else {
            return;
        };
        let Some(sigma_f) = num::to_f32(sigma_dev) else { return };
        key.blur_q = num::to_u32_round(sigma_f * 16.0).unwrap_or(0).max(1);
        key.blur_style = style as u8;
        source.blur = Some((sigma_f, style));
        let run = num::index_u32(self.plan.mask_sources.len());
        self.plan.mask_sources.push(source);
        let start = num::index_u32(self.plan.glyphs.len());
        self.plan.glyphs.push(GlyphRequest { key: key.into(), run, origin, color: premul, linear: None });
        self.stats.tier_b_draws += 1;
        self.push(DrawCmd::Glyphs { glyphs: start..start + 1, clip: self.clips.current() }, idx);
    }

    /// The union of a run's glyph outlines in run space (each at its pen), for `BlurredGlyphs`.
    pub(super) fn run_outline(
        &mut self,
        font: &peniko::FontData,
        font_size: f32,
        coords: &[i16],
        glyph_transform: Option<Affine>,
        glyphs: &[Glyph],
    ) -> BezPath {
        let g = glyph_transform.unwrap_or(Affine::IDENTITY);
        let mut union = BezPath::new();
        for glyph in glyphs {
            let Ok(id) = u16::try_from(glyph.id) else { continue };
            let Some(path) = self.outlines.cached(font, id, font_size, coords) else { continue };
            let t = Affine::translate((f64::from(glyph.x), f64::from(glyph.y))) * g * FLIP_Y;
            union.extend((t * (*path).clone()).elements().iter().copied());
        }
        union
    }
}
