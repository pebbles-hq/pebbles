//! Tier B path masks in prepare (02 §5.2, R15): an eligible fill/stroke becomes one coverage
//! request keyed by `MaskKey`; `execute/resolve.rs` rasterizes it (R14) only on an atlas miss.

use std::hash::{DefaultHasher, Hash, Hasher};

use kurbo::{Affine, BezPath, Shape as _, Stroke};
use pebbles_render::paint::display_list::Geometry;

use super::path_strategy::{Tier, choose_tier};
use super::plan::{DrawCmd, GlyphRequest};
use super::stroke::{DashPlan, plan_dashes};
use super::{Walk, num};
use crate::execute::resources::atlas_alloc::MaskKey;

/// Tolerance for converting geometry to a path and for stroke/dash expansion (device px).
pub(crate) const GEOMETRY_TOL_PX: f64 = 1.0 / 1024.0;

/// What to rasterize on an atlas miss: `path` (local space) under `device`, whose translation
/// is only the quarter-pixel fraction of the op's (the integer part is the request origin).
#[derive(Clone, Debug)]
pub(crate) struct MaskSource {
    pub path: BezPath,
    pub device: Affine,
    pub rule: peniko::Fill,
    /// Stroke style in local space (hairline rule already applied); `None` for fills.
    pub stroke: Option<Stroke>,
    /// Blurred masks (R42): device σ and style.
    pub blur: Option<(f32, pebbles_render::paint::display_list::BlurStyle)>,
}

fn hash_geometry(g: &Geometry, h: &mut DefaultHasher) {
    let mut f = |v: f64| v.to_bits().hash(h);
    match g {
        Geometry::Rect(r) => [0.0, r.x0, r.y0, r.x1, r.y1].into_iter().for_each(&mut f),
        Geometry::RoundedRect(r) => {
            let (b, c) = (r.rect(), r.radii());
            [1.0, b.x0, b.y0, b.x1, b.y1, c.top_left, c.top_right, c.bottom_right, c.bottom_left]
                .into_iter()
                .for_each(&mut f);
        }
        Geometry::Circle(c) => [2.0, c.center.x, c.center.y, c.radius].into_iter().for_each(&mut f),
        Geometry::Path(p) => {
            f(3.0);
            for el in p.elements() {
                std::mem::discriminant(el).hash(h);
                let pts: &[kurbo::Point] = match el {
                    kurbo::PathEl::MoveTo(a) | kurbo::PathEl::LineTo(a) => std::slice::from_ref(a),
                    kurbo::PathEl::QuadTo(a, b) => &[*a, *b],
                    kurbo::PathEl::CurveTo(a, b, c) => &[*a, *b, *c],
                    kurbo::PathEl::ClosePath => &[],
                };
                for pt in pts {
                    pt.x.to_bits().hash(h);
                    pt.y.to_bits().hash(h);
                }
            }
        }
        _ => f(4.0),
    }
}

fn hash_stroke(s: &Stroke) -> u64 {
    let mut h = DefaultHasher::new();
    [s.width, s.miter_limit, s.dash_offset].iter().for_each(|v| v.to_bits().hash(&mut h));
    s.dash_pattern.iter().for_each(|v| v.to_bits().hash(&mut h));
    (s.join as u8, s.start_cap as u8, s.end_cap as u8).hash(&mut h);
    h.finish() | 1 // 0 is reserved for fills
}

/// Subpixel bins per device px for a mask's translation: 1/4096 px, finer than zeno's own
/// 1/256-px fixed point, so binning never adds a second rounding. Quarter-pixel bins shifted
/// edges by up to 1/8 px (≈ 30 LSB against the golden model).
const SUBPX_BINS: f64 = 4096.0;

/// Split a device translation into an integer origin and a 1/4096-px bin (0..=4095).
fn split_translation(t: f64) -> Option<(f64, u16)> {
    let q = (t * SUBPX_BINS).round();
    if !q.is_finite() || q.abs() > 1.0e12 {
        return None;
    }
    let origin = (q / SUBPX_BINS).floor();
    let bin = q - origin * SUBPX_BINS;
    Some((origin, u16::try_from(num::to_u64_sat(bin)).ok()?))
}

/// The key, source, origin and color scale of a Tier B request; `None` when the op must stay
/// on the tessellated path (non-finite placement, or a dash pattern the guard rewrites).
impl Default for MaskSource {
    fn default() -> Self {
        Self {
            path: BezPath::new(),
            device: Affine::IDENTITY,
            rule: peniko::Fill::NonZero,
            stroke: None,
            blur: None,
        }
    }
}

/// The frame's mask sources, pooled (M18): slots keep their path allocations across frames.
#[derive(Default)]
pub(crate) struct MaskSources {
    items: Vec<MaskSource>,
    len: usize,
}

impl MaskSources {
    pub(crate) fn clear(&mut self) {
        self.len = 0;
    }

    pub(crate) fn get(&self, i: usize) -> Option<&MaskSource> {
        self.items.get(..self.len)?.get(i)
    }

    /// The next slot, emptied (its path allocation is reused); `commit` keeps it.
    pub(crate) fn next_slot(&mut self) -> &mut MaskSource {
        if self.items.len() == self.len {
            self.items.push(MaskSource::default());
        }
        #[allow(clippy::indexing_slicing)] // `len < items.len()`: pushed above
        let s = &mut self.items[self.len];
        s.path.truncate(0);
        s.stroke = None;
        s.blur = None;
        s
    }

    /// Keep the slot `next_slot` returned; its index.
    pub(crate) fn commit(&mut self) -> u32 {
        self.len += 1;
        num::index_u32(self.len - 1)
    }
}

/// `shape` flattened at `tol` into `out` (no allocation once `out` has the capacity).
fn append_path(shape: &Geometry, tol: f64, out: &mut BezPath) {
    match shape {
        Geometry::Rect(r) => out.extend(r.path_elements(tol)),
        Geometry::RoundedRect(r) => out.extend(r.path_elements(tol)),
        Geometry::Circle(c) => out.extend(c.path_elements(tol)),
        Geometry::Path(p) => out.extend(p.elements().iter().copied()),
        other => out.extend(other.to_path(tol).elements().iter().copied()),
    }
}

/// Build the mask key into `slot` (path, device map, rule, stroke); `None` when the shape must be
/// tessellated instead.
pub(super) fn build(
    shape: &Geometry,
    xf: Affine,
    rule: peniko::Fill,
    stroke: Option<&Stroke>,
    slot: &mut MaskSource,
) -> Option<(MaskKey, [f32; 2], f32)> {
    let [a, b, c, d, e, f] = xf.as_coeffs();
    let (ox, bx) = split_translation(e)?;
    let (oy, by) = split_translation(f)?;
    let q = |v: f64| {
        i32::try_from(num::to_u64_sat((v * 1024.0).round().abs())).ok().map(|m| if v < 0.0 { -m } else { m })
    };
    let linear_q = [q(a)?, q(b)?, q(c)?, q(d)?];
    let device = Affine::new([a, b, c, d, f64::from(bx) / SUBPX_BINS, f64::from(by) / SUBPX_BINS]);
    let s = xf.spectral_norm().max(1e-9);
    // 1/1024 device px: the converted curves (and so dash positions) match the spec's geometry.
    append_path(shape, GEOMETRY_TOL_PX / s, &mut slot.path);
    let path = &slot.path;
    let mut alpha = 1.0f32;
    let stroke = match stroke {
        None => None,
        Some(style) => {
            if !matches!(plan_dashes(style, path, s), DashPlan::Undashed | DashPlan::Dashed) {
                return None;
            }
            // Hairline rule (02 §5.5): exactly 1 device px, the width folded into alpha.
            let wd = style.width * xf.determinant().abs().sqrt();
            let mut st = style.clone();
            if wd > 0.0 && wd < 1.0 {
                st.width = style.width / wd;
                alpha = num::to_f32(wd)?;
            }
            Some(st)
        }
    };
    let mut h = DefaultHasher::new();
    hash_geometry(shape, &mut h);
    let key = MaskKey {
        geometry_hash: h.finish(),
        linear_q,
        subpx: (bx, by),
        rule: u8::from(rule == peniko::Fill::EvenOdd && stroke.is_none()),
        stroke_hash: stroke.as_ref().map_or(0, hash_stroke),
        blur_q: 0,
        blur_style: 0,
    };
    let rule = if stroke.is_some() { peniko::Fill::NonZero } else { rule };
    let origin = [num::to_f32(ox)?, num::to_f32(oy)?];
    (slot.device, slot.rule, slot.stroke) = (device, rule, stroke);
    Some((key, origin, alpha))
}

impl Walk<'_> {
    /// The op's tier (R16); everything is tessellated when Tier A is switched off (tests).
    pub(super) fn tier(&self, op: &pebbles_render::paint::display_list::Op, xf: Affine) -> Tier {
        if self.sdf { choose_tier(op, xf) } else { Tier::C }
    }

    /// Draw a Tier B fill/stroke as a coverage-atlas request; `false` if it must be tessellated.
    pub(super) fn draw_mask(
        &mut self,
        shape: &Geometry,
        xf: Affine,
        rule: peniko::Fill,
        stroke: Option<&Stroke>,
        premul: [f32; 4],
        idx: u32,
    ) -> bool {
        let slot = self.plan.mask_sources.next_slot();
        let Some((key, origin, alpha)) = build(shape, xf, rule, stroke, slot) else { return false };
        let run = self.plan.mask_sources.commit();
        let start = num::index_u32(self.plan.glyphs.len());
        let color = premul.map(|v| v * alpha);
        self.plan.glyphs.push(GlyphRequest { key: key.into(), run, origin, color, linear: None });
        self.stats.tier_b_draws += 1;
        self.push(DrawCmd::Glyphs { glyphs: start..start + 1, clip: self.clips.current() }, idx);
        true
    }
}
