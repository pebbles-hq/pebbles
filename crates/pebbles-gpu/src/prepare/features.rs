//! Op-coverage bookkeeping (R01): mark which OC rows an op exercises that the renderer does
//! not deliver exactly yet. Pure bookkeeping — drawing is unchanged; C17 still records the
//! `Unsupported` kinds that are logged.

use kurbo::Affine;
use pebbles_render::paint::display_list::Geometry;

use super::Walk;
use super::coverage::Feature;

/// A transform that maps axis-aligned rects to axis-aligned rects (0°/90°/180°/270°).
fn quarter_turn(xf: Affine) -> bool {
    let c = xf.as_coeffs();
    (c[1].abs() < 1e-9 && c[2].abs() < 1e-9) || (c[0].abs() < 1e-9 && c[3].abs() < 1e-9)
}

/// A rect, or a rounded rect with zero radii.
fn is_rect_like(g: &Geometry) -> bool {
    match g {
        Geometry::Rect(_) => true,
        Geometry::RoundedRect(rr) => {
            let r = rr.radii();
            [r.top_left, r.top_right, r.bottom_right, r.bottom_left].iter().all(|v| v.abs() < 1e-9)
        }
        _ => false,
    }
}

impl Walk<'_> {
    pub(super) fn fell_back(&mut self, f: Feature) {
        self.plan.fallback_features.insert(f);
    }

    /// A non-solid fill or stroke brush (skipped by C17): its gradient kind / image rows.
    pub(super) fn note_paint(&mut self, brush: &peniko::Brush, brush_xf: Option<Affine>, stroke: bool) {
        match brush {
            peniko::Brush::Solid(_) => {}
            // Gradients draw exactly and dithered (L35, L37).
            peniko::Brush::Gradient(_) => {}
            peniko::Brush::Image(_) => {
                self.fell_back(if stroke { Feature::StrokeImageBrush } else { Feature::FillImageBrush });
                if brush_xf.is_some() {
                    self.fell_back(Feature::ImageBrushTransform);
                }
            }
        }
    }

    /// The clip rows of a layer clip (quarter turns and zero-radius rounded rects are exact
    /// scissors since R07).
    pub(super) fn note_clip(
        &mut self,
        clip: &Geometry,
        xf: Affine,
        fill: peniko::Fill,
        kind: &super::clip::ClipKind,
    ) {
        use super::clip::{ClipClass, ClipKind, clip_class};
        // Every clip route is exact (scissor, SDF L04, Tier B mask L08, GPU mask L09) except the
        // row-10 overflow, which clips to the bounding box.
        if !matches!(kind, ClipKind::Overflow) {
            return;
        }
        match (clip_class(clip, xf), clip) {
            (ClipClass::Scissor { .. }, _) => {}
            (_, Geometry::Rect(_) | Geometry::RoundedRect(_)) if !quarter_turn(xf) && is_rect_like(clip) => {
                self.fell_back(Feature::ClipRectRotated);
            }
            (_, Geometry::RoundedRect(_)) => self.fell_back(Feature::ClipRRect),
            (_, Geometry::Circle(_)) => self.fell_back(Feature::ClipCircle),
            _ => {
                self.fell_back(Feature::ClipPath);
            }
        }
        self.fell_back(Feature::ClipGpuMask);
        if fill == peniko::Fill::EvenOdd && !matches!(clip, Geometry::Rect(_)) {
            self.fell_back(Feature::ClipEvenOdd);
        }
    }

    /// Glyph-run rows decided in prepare (colour glyphs are only known at raster time).
    /// Non-solid glyph brushes (L36). Transformed, rotated and huge runs take the outline
    /// route since R20.
    pub(super) fn note_glyphs(&mut self, brush: &peniko::Brush) {
        // Gradient runs are painted through their outlines (L36); image brushes are L41's.
        if matches!(brush, peniko::Brush::Image(_)) {
            self.fell_back(Feature::GlyphsNonSolidBrush);
        }
    }
}
