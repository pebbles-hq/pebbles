//! Tier B path masks in prepare (02 §5.2, R15): an eligible fill/stroke becomes one coverage
//! request keyed by `MaskKey`; `execute/resolve.rs` rasterizes it (R14) only on an atlas miss.

use std::hash::{DefaultHasher, Hash, Hasher};

use kurbo::{Affine, BezPath, Stroke};
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

/// Split a device translation into an integer origin and a quarter-pixel bin (0..=3).
fn split_translation(t: f64) -> Option<(f64, u8)> {
    let q = (t * 4.0).round();
    if !q.is_finite() || q.abs() > 4.0e9 {
        return None;
    }
    let origin = (q / 4.0).floor();
    let bin = q - origin * 4.0;
    Some((origin, u8::try_from(num::to_u64_sat(bin)).ok()?))
}

/// The key, source, origin and color scale of a Tier B request; `None` when the op must stay
/// on the tessellated path (non-finite placement, or a dash pattern the guard rewrites).
fn build(
    shape: &Geometry,
    xf: Affine,
    rule: peniko::Fill,
    stroke: Option<&Stroke>,
) -> Option<(MaskKey, MaskSource, [f32; 2], f32)> {
    let [a, b, c, d, e, f] = xf.as_coeffs();
    let (ox, bx) = split_translation(e)?;
    let (oy, by) = split_translation(f)?;
    let q = |v: f64| {
        i32::try_from(num::to_u64_sat((v * 1024.0).round().abs())).ok().map(|m| if v < 0.0 { -m } else { m })
    };
    let linear_q = [q(a)?, q(b)?, q(c)?, q(d)?];
    let device = Affine::new([a, b, c, d, f64::from(bx) / 4.0, f64::from(by) / 4.0]);
    let s = xf.spectral_norm().max(1e-9);
    // 1/1024 device px: the converted curves (and so dash positions) match the spec's geometry.
    let path = shape.to_path(GEOMETRY_TOL_PX / s).into_owned();
    let mut alpha = 1.0f32;
    let stroke = match stroke {
        None => None,
        Some(style) => {
            if !matches!(plan_dashes(style, &path, s), DashPlan::Undashed | DashPlan::Dashed) {
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
    };
    let rule = if stroke.is_some() { peniko::Fill::NonZero } else { rule };
    let origin = [num::to_f32(ox)?, num::to_f32(oy)?];
    Some((key, MaskSource { path, device, rule, stroke }, origin, alpha))
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
        let Some((key, source, origin, alpha)) = build(shape, xf, rule, stroke) else { return false };
        let run = num::index_u32(self.plan.mask_sources.len());
        self.plan.mask_sources.push(source);
        let start = num::index_u32(self.plan.glyphs.len());
        let color = premul.map(|v| v * alpha);
        self.plan.glyphs.push(GlyphRequest { key: key.into(), run, origin, color });
        self.stats.tier_b_draws += 1;
        self.push(DrawCmd::Glyphs { glyphs: start..start + 1, clip: self.clips.current() }, idx);
        true
    }
}
