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

/// Translation + uniform positive scale (the glyph atlas route, 02 §7.5).
fn uniform_scale(xf: Affine) -> bool {
    let [a, b, c, d, ..] = xf.as_coeffs();
    let s = a.abs().max(d.abs());
    b.abs() <= 1e-6 * s && c.abs() <= 1e-6 * s && a > 0.0 && d > 0.0 && (a - d).abs() <= 1e-6 * a
}

impl Walk<'_> {
    fn fell_back(&mut self, f: Feature) {
        self.plan.fallback_features.insert(f);
    }

    /// A non-solid fill or stroke brush (skipped by C17): its gradient kind / image rows.
    pub(super) fn note_paint(&mut self, brush: &peniko::Brush, brush_xf: Option<Affine>, stroke: bool) {
        match brush {
            peniko::Brush::Solid(_) => {}
            peniko::Brush::Gradient(g) => {
                self.fell_back(if stroke {
                    Feature::StrokeGradient
                } else {
                    match g.kind {
                        peniko::GradientKind::Linear(_) => Feature::FillGradientLinear,
                        peniko::GradientKind::Radial(_) => Feature::FillGradientRadial,
                        peniko::GradientKind::Sweep(_) => Feature::FillGradientSweep,
                    }
                });
                if g.extend != peniko::Extend::Pad || g.stops.iter().any(|s| s.color.components[3] < 1.0) {
                    self.fell_back(Feature::GradientStopsExtend);
                }
                if g.interpolation_cs != peniko::color::ColorSpaceTag::Srgb {
                    self.fell_back(Feature::GradientNonSrgbInterpolation);
                }
                if brush_xf.is_some() {
                    self.fell_back(Feature::GradientBrushTransform);
                }
                self.fell_back(Feature::GradientDither);
            }
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
    pub(super) fn note_clip(&mut self, clip: &Geometry, xf: Affine, fill: peniko::Fill) {
        use super::clip::{ClipClass, clip_class};
        match (clip_class(clip, xf), clip) {
            (ClipClass::Scissor { .. }, _) => {}
            (_, Geometry::Rect(_) | Geometry::RoundedRect(_)) if !quarter_turn(xf) && is_rect_like(clip) => {
                self.fell_back(Feature::ClipRectRotated);
            }
            (_, Geometry::RoundedRect(_)) => self.fell_back(Feature::ClipRRect),
            (_, Geometry::Circle(_)) => self.fell_back(Feature::ClipCircle),
            _ => {
                self.fell_back(Feature::ClipPath);
                // A path clip covering the whole target needs the GPU mask route (L09).
                let b = xf.transform_rect_bbox(clip.bounding_box());
                let (w, h) = self.plan.target_size;
                if b.width() >= f64::from(w) && b.height() >= f64::from(h) {
                    self.fell_back(Feature::ClipGpuMask);
                }
            }
        }
        if fill == peniko::Fill::EvenOdd && !matches!(clip, Geometry::Rect(_)) {
            self.fell_back(Feature::ClipEvenOdd);
        }
    }

    /// Group alpha and blend rows of a layer.
    pub(super) fn note_layer(&mut self, alpha: f32, blend: peniko::BlendMode) {
        if alpha > 0.0 && alpha < 1.0 {
            self.fell_back(Feature::LayerAlpha);
        }
        let mix = blend.mix != peniko::Mix::Normal;
        let compose = blend.compose != peniko::Compose::SrcOver;
        match (mix, compose) {
            (true, true) => self.fell_back(Feature::LayerMixAndCompose),
            (true, false) => self.fell_back(Feature::LayerMix),
            (false, true) => self.fell_back(Feature::LayerCompose),
            (false, false) => {}
        }
    }

    /// Glyph-run rows decided in prepare (colour glyphs are only known at raster time).
    pub(super) fn note_glyphs(
        &mut self,
        brush: &peniko::Brush,
        glyph_xf: Option<Affine>,
        xf: Affine,
        size: f32,
    ) {
        if !matches!(brush, peniko::Brush::Solid(_)) {
            self.fell_back(Feature::GlyphsNonSolidBrush);
        }
        if glyph_xf.is_some_and(|g| g != Affine::IDENTITY) {
            self.fell_back(Feature::GlyphsTransform);
        }
        if !uniform_scale(xf) {
            self.fell_back(Feature::GlyphsRotated);
        } else if f64::from(size) * xf.as_coeffs()[0] > 256.0 {
            self.fell_back(Feature::GlyphsLarge);
        }
    }

    /// Image sampler and placement rows (R30–R35 deliver them).
    pub(super) fn note_image(&mut self, image: &peniko::ImageBrush, xf: Affine) {
        let s = &image.sampler;
        // Medium (the default) is the bilinear sampling drawn today; Low/High are R30's.
        if s.quality != peniko::ImageQuality::Medium {
            self.fell_back(Feature::ImageQuality);
        }
        // Filtered minification needs mipmaps (R35).
        if s.quality != peniko::ImageQuality::Low && xf.determinant().abs() < 1.0 {
            self.fell_back(Feature::ImageMipmaps);
        }
        if s.x_extend != peniko::Extend::Pad || s.y_extend != peniko::Extend::Pad {
            self.fell_back(Feature::ImageExtend);
        }
        if s.alpha < 1.0 {
            self.fell_back(Feature::ImageAlpha);
        }
        if !quarter_turn(xf) || xf.as_coeffs()[0] < 0.0 || xf.as_coeffs()[3] < 0.0 {
            self.fell_back(Feature::ImageTransform);
        }
    }

    /// Shadow rows (R40/R41 deliver them).
    pub(super) fn note_shadow(&mut self, xf: Affine) {
        let c = xf.as_coeffs();
        let axis_aligned = c[1].abs() < 1e-9 && c[2].abs() < 1e-9;
        self.fell_back(if axis_aligned { Feature::ShadowAxisAligned } else { Feature::ShadowRotated });
    }
}
