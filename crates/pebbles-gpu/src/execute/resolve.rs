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
    queue: &wgpu::Queue,
    plan: &FramePlan,
    out: &mut Vec<QuadInstance>,
    stats: &mut FrameStats,
) {
    out.clear();
    for req in &plan.glyphs {
        let key = match req.key {
            CoverageKey::Glyph(k) => k,
            CoverageKey::Path(k) => {
                let Some(src) = plan.mask_sources.get(req.run as usize) else {
                    out.push(QuadInstance::zeroed());
                    continue;
                };
                let (lookup, rasterized) = atlas.lookup_path(queue, k, src);
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
        };
        let (lookup, rasterized) = atlas.lookup(queue, key, &raster);
        stats.glyphs_rasterized += u32::from(rasterized);
        out.push(quad_for(lookup, req.origin, req.color, stats));
    }
}

/// R25 step 2: a resolved glyph quad that misses its command's scissor becomes zero-area (the
/// exact quad of a resident glyph is known only after the atlas lookup).
pub(crate) fn cull_resolved(plan: &FramePlan, out: &mut [QuadInstance]) {
    for cmd in &plan.cmds {
        let crate::prepare::DrawCmd::Glyphs { glyphs, clip } = cmd else { continue };
        let s = clip.scissor;
        for q in out.get_mut(glyphs.start as usize..glyphs.end as usize).unwrap_or_default() {
            let (x0, y0) = (q.origin[0], q.origin[1]);
            let (x1, y1) = (x0 + q.xform[0] + q.xform[2], y0 + q.xform[1] + q.xform[3]);
            let (lo_x, hi_x, lo_y, hi_y) = (x0.min(x1), x0.max(x1), y0.min(y1), y0.max(y1));
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
