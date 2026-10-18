//! Glyph-run preparation: one `GlyphRequest` per glyph, with the pen snapped to the device
//! pixel grid (02 §7.4) and the atlas key built exactly as 10 §5.2.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;

use kurbo::Affine;
use peniko::FontData;

use pebbles_render::paint::Glyph;

use super::num::{index_u32, to_f32};
use super::plan::{FramePlan, GlyphRequest, GlyphRunInfo, ScissorRect};
use crate::execute::resources::atlas_alloc::GlyphKey;
use crate::stats::{FrameStats, Unsupported};

/// Device pen x → (whole pixel, quarter-pixel bin 0..=3). Nearest quarter; a bin of 4 carries.
#[allow(clippy::cast_possible_truncation)] // bounded by the |x| ≤ 2^24 guard
pub(crate) fn subpixel_bin(x: f32) -> Option<(i32, u8)> {
    if !x.is_finite() || x.abs() > 16_777_216.0 {
        return None;
    }
    let q = (x * 4.0).round() as i64;
    Some((q.div_euclid(4) as i32, q.rem_euclid(4) as u8))
}

/// Device baseline y → whole pixel (round, not trunc: -0.7 → -1, 0.5 → 1).
#[allow(clippy::cast_possible_truncation)] // bounded by the |y| ≤ 2^24 guard
pub(crate) fn snap_baseline(y: f32) -> Option<i32> {
    (y.is_finite() && y.abs() <= 16_777_216.0).then(|| y.round() as i32)
}

/// 0 for no coords, else a 64-bit hash of them (10 §5.2).
fn coords_hash(coords: &[i16]) -> u64 {
    if coords.is_empty() {
        return 0;
    }
    let mut h = DefaultHasher::new();
    coords.hash(&mut h);
    h.finish().max(1)
}

/// R25: whether a glyph's conservative device box `[x − 2·ppem, x + 3·ppem] × [y ± 2·ppem]`
/// around `pen` (mapped by `linear` for the outline route) misses `scissor`.
pub(crate) fn glyph_culled(pen: kurbo::Point, ppem: f64, linear: Affine, scissor: ScissorRect) -> bool {
    let local = kurbo::Rect::new(-2.0 * ppem, -2.0 * ppem, 3.0 * ppem, 2.0 * ppem);
    let b = linear.transform_rect_bbox(local) + pen.to_vec2();
    let s = super::cull::scissor_rect(scissor);
    b.x1 <= s.x0 || b.x0 >= s.x1 || b.y1 <= s.y0 || b.y0 >= s.y1
}

/// Append one atlas-route glyph run's requests (pens in device space) and return their range.
/// Degenerate glyphs are skipped and counted (R22); glyphs outside `scissor` are culled before
/// any atlas work (R25).
#[allow(clippy::too_many_arguments)]
pub(crate) fn glyph_run(
    plan: &mut FramePlan,
    stats: &mut FrameStats,
    scissor: ScissorRect,
    font: &FontData,
    font_size: f32,
    coords: &[i16],
    color: [f32; 4],
    transform: Affine,
    glyphs: &[Glyph],
) -> Range<u32> {
    let start = index_u32(plan.glyphs.len());
    let coeffs = transform.as_coeffs();
    let scale = to_f32((coeffs[0] * coeffs[0] + coeffs[1] * coeffs[1]).sqrt().max(1e-6)).unwrap_or(0.0);
    let size_px = font_size * scale;
    // R22 step 2: below half a device pixel nothing is visible.
    if size_px < 0.5 {
        return start..start;
    }
    let Some(size_q) = super::num::to_u32_round(size_px * 64.0) else { return start..start };
    if size_q == 0 {
        return start..start;
    }
    let run = index_u32(plan.glyph_runs.len());
    let c0 = index_u32(plan.coords.len());
    plan.coords.extend_from_slice(coords);
    plan.glyph_runs.push(GlyphRunInfo {
        font: font.clone(),
        coords: c0..index_u32(plan.coords.len()),
        size_px,
    });
    let hash = coords_hash(coords);
    for g in glyphs {
        let Ok(glyph_id) = u16::try_from(g.id) else {
            plan.fallback_features.insert(super::coverage::Feature::GlyphsDegenerate);
            stats.record(Unsupported::GlyphIdRange);
            continue;
        };
        let pen = transform * kurbo::Point::new(f64::from(g.x), f64::from(g.y));
        let (Some((ix, subpx_x)), Some(iy)) =
            (to_f32(pen.x).and_then(subpixel_bin), to_f32(pen.y).and_then(snap_baseline))
        else {
            stats.non_finite_skipped += 1;
            stats.record(Unsupported::NonFinite);
            continue;
        };
        if glyph_culled(pen, f64::from(size_px), Affine::IDENTITY, scissor) {
            plan.stats.culled_glyphs += 1;
            continue;
        }
        let key = GlyphKey {
            font_blob: font.data.id(),
            font_index: font.index,
            glyph_id,
            size_q,
            subpx_x,
            hint: false,
            coords_hash: hash,
        };
        #[allow(clippy::cast_precision_loss)] // |ix|, |iy| ≤ 2^24: exact in f32
        let origin = [ix as f32, iy as f32];
        plan.glyphs.push(GlyphRequest { key: key.into(), run, origin, color });
    }
    start..index_u32(plan.glyphs.len())
}

#[cfg(test)]
#[path = "glyphs_tests.rs"]
mod run_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subpixel_bin_rounds_to_nearest_quarter_and_carries() {
        assert_eq!(subpixel_bin(10.0), Some((10, 0)));
        assert_eq!(subpixel_bin(10.3), Some((10, 1)));
        assert_eq!(subpixel_bin(10.9), Some((11, 0)));
        assert_eq!(subpixel_bin(-0.3), Some((-1, 3)));
        assert_eq!(subpixel_bin(f32::NAN), None);
    }

    #[test]
    fn baseline_rounds() {
        assert_eq!(snap_baseline(-0.7), Some(-1));
        assert_eq!(snap_baseline(0.5), Some(1));
        assert_eq!(snap_baseline(f32::INFINITY), None);
    }
}
