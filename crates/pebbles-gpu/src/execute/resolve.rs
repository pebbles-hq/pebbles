//! Glyph resolution: each `GlyphRequest` → one `QuadInstance`, rasterizing and uploading only
//! on an atlas miss. A glyph with no slot becomes a zero-area quad so ranges stay valid.

use super::resources::atlas_alloc::{AtlasKind, AtlasSlot, CoverageKey};
use super::resources::glyph_atlas::{GlyphAtlas, GlyphLookup};
use crate::gpu_types::QuadInstance;
use crate::prepare::FramePlan;
use crate::raster::swash::RasterRequest;
use crate::stats::{FrameStats, Unsupported};

/// Resolve every glyph request of `plan` into `out` (index-aligned with `plan.glyphs`).
pub(crate) fn resolve_glyphs(
    atlas: &mut GlyphAtlas,
    plan: &FramePlan,
    out: &mut Vec<QuadInstance>,
    stats: &mut FrameStats,
) {
    out.clear();
    for req in &plan.glyphs {
        let key = match req.key {
            CoverageKey::Glyph(k) => k,
            // Mask clips are resolved per record (09 L08), never as glyph requests.
            CoverageKey::Clip(..) => {
                out.push(QuadInstance::zeroed());
                continue;
            }
            CoverageKey::Path(k) => {
                let Some(src) = plan.mask_sources.get(req.run as usize) else {
                    out.push(QuadInstance::zeroed());
                    continue;
                };
                let (lookup, rasterized) = atlas.lookup_path(k, src, stats);
                stats.masks_rasterized += u32::from(rasterized);
                out.push(quad_for(lookup, req.origin, req.color, stats));
                continue;
            }
        };
        let Some(run) = plan.glyph_runs.get(req.run as usize) else {
            out.push(QuadInstance::zeroed());
            continue;
        };
        let coords = plan.coords.get(run.coords.start as usize..run.coords.end as usize).unwrap_or_default();
        let raster = RasterRequest {
            font: &run.font,
            glyph_id: key.glyph_id,
            size_px: run.size_px,
            subpx_x: key.subpx_x,
            coords,
            hint: key.hint,
            native_strike: req.linear.is_some(),
        };
        let (lookup, rasterized) = atlas.lookup(key, &raster, stats);
        stats.glyphs_rasterized += u32::from(rasterized);
        let quad = match lookup {
            GlyphLookup::Slot(slot) if req.linear.is_some() || slot.strike_ppem > 0 => {
                strike_quad(slot, req, run.size_px)
            }
            other => quad_for(other, req.origin, req.color, stats),
        };
        out.push(quad);
    }
}

/// R25 step 2: a resolved glyph quad that misses its command's scissor becomes zero-area (the
/// exact quad of a resident glyph is known only after the atlas lookup).
pub(crate) fn cull_resolved(plan: &FramePlan, out: &mut [QuadInstance]) {
    for cmd in &plan.cmds {
        let crate::prepare::DrawCmd::Glyphs { glyphs, clip } = cmd else { continue };
        let s = clip.scissor;
        for q in out.get_mut(glyphs.start as usize..glyphs.end as usize).unwrap_or_default() {
            // All four corners: transformed strike quads (R23) are parallelograms.
            let [ox, oy] = q.origin;
            let [a, b, c, d] = q.xform;
            let xs = [ox, ox + a, ox + c, ox + a + c];
            let ys = [oy, oy + b, oy + d, oy + b + d];
            let (lo_x, hi_x) =
                (xs.iter().copied().fold(f32::MAX, f32::min), xs.iter().copied().fold(f32::MIN, f32::max));
            let (lo_y, hi_y) =
                (ys.iter().copied().fold(f32::MAX, f32::min), ys.iter().copied().fold(f32::MIN, f32::max));
            #[allow(clippy::cast_precision_loss)] // scissor edges ≤ 2^24
            let miss =
                hi_x <= s.x0 as f32 || lo_x >= s.x1 as f32 || hi_y <= s.y0 as f32 || lo_y >= s.y1 as f32;
            if miss {
                *q = QuadInstance::zeroed();
            }
        }
    }
}

fn quad_for(lookup: GlyphLookup, origin: [f32; 2], color: [f32; 4], stats: &mut FrameStats) -> QuadInstance {
    match lookup {
        GlyphLookup::Slot(slot) => glyph_quad(slot, origin, color),
        GlyphLookup::Empty => QuadInstance::zeroed(),
        GlyphLookup::Full => {
            stats.record(Unsupported::AtlasFull);
            QuadInstance::zeroed()
        }
        GlyphLookup::Unsupported => {
            stats.record(Unsupported::GlyphContent);
            QuadInstance::zeroed()
        }
    }
}

/// A native-size strike (R23): scaled `size / strike_ppem`; on the outline route also mapped by
/// the run's linear part about the unsnapped pen (step 2a), else an axis-aligned quad with
/// rounded edges (step 1b).
fn strike_quad(slot: AtlasSlot, req: &crate::prepare::GlyphRequest, size: f32) -> QuadInstance {
    let s = if slot.strike_ppem > 0 { size / f32::from(slot.strike_ppem) } else { 1.0 };
    let (w, h) = (f32::from(slot.w), f32::from(slot.h));
    let (x, y) = (f32::from(slot.x), f32::from(slot.y));
    let uv = [x, y, x + w, y + h];
    let layer_kind = [u32::from(slot.layer), u32::from(slot.kind == AtlasKind::Color)];
    let [ox, oy] = req.origin;
    let Some([a, b, c, d]) = req.linear else {
        let [x0, y0, x1, y1] = crate::raster::bitmap::scaled_strike_quad(
            ox,
            oy,
            slot.left.into(),
            slot.top.into(),
            slot.w.into(),
            slot.h.into(),
            s,
        );
        return QuadInstance {
            xform: [x1 - x0, 0.0, 0.0, y1 - y0],
            origin: [x0, y0],
            uv,
            color: req.color,
            layer_kind,
        };
    };
    // Glyph space (y-down): the bitmap's top-left is at (left, −top)·s from the pen.
    let (lx, ly) = (f32::from(slot.left) * s, -f32::from(slot.top) * s);
    QuadInstance {
        xform: [a * w * s, b * w * s, c * h * s, d * h * s],
        origin: [ox + a * lx + c * ly, oy + b * lx + d * ly],
        uv,
        color: req.color,
        layer_kind,
    }
}

/// Place a resident glyph: `x0 = ix + left`, `y0 = iy − top`, size = bitmap size (02 §7.4).
fn glyph_quad(slot: AtlasSlot, origin: [f32; 2], color: [f32; 4]) -> QuadInstance {
    let (w, h) = (f32::from(slot.w), f32::from(slot.h));
    let (x, y) = (f32::from(slot.x), f32::from(slot.y));
    QuadInstance {
        xform: [w, 0.0, 0.0, h],
        origin: [origin[0] + f32::from(slot.left), origin[1] - f32::from(slot.top)],
        uv: [x, y, x + w, y + h],
        color,
        layer_kind: [u32::from(slot.layer), u32::from(slot.kind == AtlasKind::Color)],
    }
}

trait Zeroed {
    fn zeroed() -> Self;
}

impl Zeroed for QuadInstance {
    fn zeroed() -> Self {
        bytemuck::Zeroable::zeroed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Affine;
    use pebbles_render::paint::{Glyph, Painter, Scene};

    /// R24: one run is one `glyphs` command whatever each glyph's kind; the kind is per instance
    /// (glyph 2 stubbed as a color-atlas slot) and the UVs stay in atlas pixels.
    #[test]
    fn mixed_mask_color_run_is_one_command() {
        let mut s = Scene::new();
        let glyphs = [40u32, 41, 42].map(|id| Glyph { id, x: f32::from(u8::try_from(id).unwrap()), y: 20.0 });
        Painter::new(&mut s).draw_glyphs(
            &crate::test_support::bundled_font(),
            16.0,
            &[],
            &peniko::Brush::Solid(peniko::Color::BLACK),
            Affine::IDENTITY,
            None,
            peniko::Fill::NonZero,
            glyphs.into_iter(),
        );
        let (plan, _) = crate::prepare::test_util::prepare_scene(&s, 128, 64);
        let cmds: Vec<_> =
            plan.cmds.iter().filter(|c| matches!(c, crate::prepare::DrawCmd::Glyphs { .. })).collect();
        assert_eq!(cmds.len(), 1);
        assert_eq!(plan.glyphs.len(), 3);
        let slot =
            |kind| AtlasSlot { kind, layer: 1, x: 10, y: 20, w: 8, h: 9, left: 0, top: 9, strike_ppem: 0 };
        let kinds = [AtlasKind::Mask, AtlasKind::Color, AtlasKind::Mask]
            .map(|k| glyph_quad(slot(k), [0.0, 0.0], [1.0; 4]).layer_kind);
        assert_eq!(kinds, [[1, 0], [1, 1], [1, 0]]);
        assert_eq!(glyph_quad(slot(AtlasKind::Color), [0.0, 0.0], [1.0; 4]).uv, [10.0, 20.0, 18.0, 29.0]);
    }
}
