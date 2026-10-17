//! Glyph resolution: each `GlyphRequest` → one `QuadInstance`, rasterizing and uploading only
//! on an atlas miss. A glyph with no slot becomes a zero-area quad so ranges stay valid.

use super::resources::atlas_alloc::{AtlasKind, AtlasSlot};
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
        let Some(run) = plan.glyph_runs.get(req.run as usize) else {
            out.push(QuadInstance::zeroed());
            continue;
        };
        let coords = plan.coords.get(run.coords.start as usize..run.coords.end as usize).unwrap_or_default();
        let raster = RasterRequest {
            font: &run.font,
            glyph_id: req.key.glyph_id,
            size_px: run.size_px,
            subpx_x: req.key.subpx_x,
            coords,
        };
        let (lookup, rasterized) = atlas.lookup(queue, req.key, &raster);
        stats.glyphs_rasterized += u32::from(rasterized);
        let quad = match lookup {
            GlyphLookup::Slot(slot) => glyph_quad(slot, req.origin, req.color),
            GlyphLookup::Empty => QuadInstance::zeroed(),
            GlyphLookup::Full => {
                stats.record(Unsupported::AtlasFull);
                QuadInstance::zeroed()
            }
            GlyphLookup::Unsupported => {
                stats.record(Unsupported::GlyphContent);
                QuadInstance::zeroed()
            }
        };
        out.push(quad);
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
