//! The golden model (05 §6.7a, DR-21): an independent CPU implementation of the raster spec,
//! written for exactness, not speed. Shares no rasterization code with pebbles-gpu.
//!
//! Op coverage (02 §2 matrix, V45 AC2): `Fill`/`Stroke` with solid, gradient (every kind and
//! extend, DR-05 interpolation) and image brushes; `PushLayer`/`PopLayer` with every clip
//! geometry, group alpha and every `Mix` × `Compose`; `Image`; `BlurredRect`; `Glyphs` (outline
//! and atlas routes, hinting, any brush); `Fragment`. Added by V51: mip chains (R35), nine-slice
//! (R36), dynamic images (R37), blurred masks (R42), bitmap-strike glyph resampling (R23) and
//! COLR glyphs (R29); by V52: effects, backdrop filters, projected layers, meshes, sprites.

pub mod colr;
pub mod compose;
pub mod coverage;
pub mod layers;
pub mod paint;
pub mod shadow;
pub mod shapes;
pub mod stroke;
pub mod text;

use kurbo::Affine;
use pebbles_render::paint::Scene;
use pebbles_render::paint::display_list::{Op, PixelSnap};
use peniko::Color;

use crate::CheckError;
use crate::metric::Rgba8;
use layers::Layer;
use paint::Paint;
use shapes::{Snap, fill_geometry, stroke_geometry};

/// The golden renderer.
pub struct Golden;

struct Target {
    w: u32,
    h: u32,
    /// The root target: premultiplied, sRGB-encoded, `f64` (02 §3).
    px: Vec<[f64; 4]>,
    /// Open layers, innermost last (09 §3; `layers.rs`).
    layers: Vec<Layer>,
    /// `Some` forces a policy for every op; `None` honors each op's `PixelSnap` (02 §5.3).
    force_snap: Option<Snap>,
    /// (device-pixel ratio, hint override) for the 02 §7.8 glyph hinting policy (R27).
    hint: (f64, Option<bool>),
}

fn premul(c: Color) -> [f64; 4] {
    let [r, g, b, a] = c.components.map(f64::from);
    [r * a, g * a, b * a, a]
}

/// `u8 = floor(clamp(v, 0, 1) · 255 + 0.5)` (02 §5.1).
fn quantize(v: f64) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8
}

impl Target {
    fn snap(&self, op_snap: PixelSnap) -> Snap {
        self.force_snap.unwrap_or(if op_snap == PixelSnap::Auto { Snap::Auto } else { Snap::Off })
    }

    /// Source-over `paint(p) · cov` into the current layer (or the root), `p` = pixel center.
    fn composite(&mut self, paint: &Paint, cov: &[f64]) {
        let w = self.w as usize;
        let buf = match self.layers.last_mut() {
            Some(l) => &mut l.px,
            None => &mut self.px,
        };
        for (i, (dst, &k)) in buf.iter_mut().zip(cov).enumerate() {
            if k == 0.0 {
                continue;
            }
            #[allow(clippy::cast_precision_loss)] // pixel indices < 2^24
            let src = paint.at((i % w) as f64 + 0.5, (i / w) as f64 + 0.5);
            let inv = 1.0 - src[3] * k;
            for ch in 0..4 {
                dst[ch] = src[ch] * k + dst[ch] * inv;
            }
        }
    }

    /// Source-over a premultiplied color image (× `alpha`) into the current layer (or the root).
    fn composite_image(&mut self, img: &[[f64; 4]], alpha: f64) {
        let buf = match self.layers.last_mut() {
            Some(l) => &mut l.px,
            None => &mut self.px,
        };
        for (dst, s) in buf.iter_mut().zip(img) {
            let s = s.map(|v| v * alpha);
            for ch in 0..4 {
                dst[ch] = s[ch] + dst[ch] * (1.0 - s[3]);
            }
        }
    }

    fn walk(&mut self, ops: &[Op], outer: Affine) -> Result<(), CheckError> {
        for op in ops {
            self.op(op, outer)?;
        }
        Ok(())
    }

    fn op(&mut self, op: &Op, outer: Affine) -> Result<(), CheckError> {
        match op {
            Op::Fill { style, transform, brush, brush_transform, shape, snap, .. } => {
                let p = outer * *transform * brush_transform.unwrap_or(Affine::IDENTITY);
                let Some(paint) = Paint::new(brush, p, 1.0)? else { return Ok(()) };
                let cov = fill_geometry(shape, outer * *transform, *style, self.snap(*snap), self.w, self.h);
                self.composite(&paint, &cov);
            }
            Op::Stroke { style, transform, brush, brush_transform, shape, snap, .. } => {
                let p = outer * *transform * brush_transform.unwrap_or(Affine::IDENTITY);
                let Some(paint) = Paint::new(brush, p, 1.0)? else { return Ok(()) };
                let cov = stroke_geometry(shape, style, outer * *transform, self.snap(*snap), self.w, self.h);
                self.composite(&paint, &cov);
            }
            Op::PushLayer { clip_fill, blend, alpha, transform, clip, .. } => {
                let mask = fill_geometry(clip, outer * *transform, *clip_fill, Snap::Off, self.w, self.h);
                let n = self.px.len();
                self.layers.push(Layer::new(n, mask, *blend, f64::from(*alpha).clamp(0.0, 1.0)));
            }
            Op::PopLayer => {
                if let Some(layer) = self.layers.pop() {
                    match self.layers.last_mut() {
                        Some(parent) => layer.pop_into(&mut parent.px),
                        None => layer.pop_into(&mut self.px),
                    }
                }
            }
            Op::Fragment { transform, ops, .. } => {
                self.walk(ops, outer * transform.unwrap_or(Affine::IDENTITY))?;
            }
            Op::Image { image, transform, snap, .. } => {
                // The image rect `(0, 0, w, h)` under the transform, painted by the image (02 §8).
                let t = outer * *transform;
                let Some(paint) = Paint::new(&peniko::Brush::Image(image.clone()), t, 1.0)? else {
                    return Ok(());
                };
                let r =
                    kurbo::Rect::new(0.0, 0.0, f64::from(image.image.width), f64::from(image.image.height));
                let shape = pebbles_render::paint::display_list::Geometry::Rect(r);
                let cov = fill_geometry(&shape, t, peniko::Fill::NonZero, self.snap(*snap), self.w, self.h);
                self.composite(&paint, &cov);
            }
            Op::BlurredRect { transform, rect, brush, radius, std_dev, .. } => {
                let cov =
                    shadow::shadow_coverage(outer * *transform, *rect, *radius, *std_dev, self.w, self.h);
                self.composite(&Paint::Solid(premul(*brush)), &cov);
            }
            Op::Glyphs {
                font,
                font_size,
                normalized_coords,
                brush,
                transform,
                glyph_transform,
                glyphs,
                ..
            } => {
                let (t, g, w, h) = (outer * *transform, *glyph_transform, self.w, self.h);
                let Some(paint) = Paint::new(brush, t, 1.0)? else { return Ok(()) };
                // 02 §7.7: color glyphs use only the run's alpha; COLR `0xFFFF` = the run color.
                let (fg, run_alpha) = match brush {
                    peniko::Brush::Solid(c) => (*c, f64::from(c.components[3])),
                    peniko::Brush::Image(i) => (Color::BLACK, f64::from(i.sampler.alpha)),
                    peniko::Brush::Gradient(_) => (Color::BLACK, 1.0),
                };
                let (hint, coords) = (self.hint, normalized_coords);
                for out in text::run_coverage(font, *font_size, coords, t, g, glyphs, w, h, hint, fg) {
                    match out {
                        text::GlyphOut::Coverage(cov) => self.composite(&paint, &cov),
                        text::GlyphOut::Color(img) => self.composite_image(&img, run_alpha),
                    }
                }
            }
            _ => return Err(CheckError::GoldenUnsupported("unknown-op")),
        }
        Ok(())
    }
}

impl Golden {
    /// Render `scene` (device px) over `background`, honoring each op's `PixelSnap`.
    pub fn render(scene: &Scene, w: u32, h: u32, background: Color) -> Result<Rgba8, CheckError> {
        Self::render_snapped(scene, w, h, background, None)
    }

    /// [`Golden::render`] for a case at device-pixel ratio `hint.0` with the `--glyph-hint`
    /// override `hint.1` (R27: hinted glyph outlines, same flag as the renderer).
    pub fn render_hinted(
        scene: &Scene,
        w: u32,
        h: u32,
        background: Color,
        hint: (f64, Option<bool>),
    ) -> Result<Rgba8, CheckError> {
        Self::render_full(scene, w, h, background, None, hint)
    }

    /// Render with one snap policy forced for every op.
    pub fn render_with(
        scene: &Scene,
        w: u32,
        h: u32,
        background: Color,
        snap: Snap,
    ) -> Result<Rgba8, CheckError> {
        Self::render_snapped(scene, w, h, background, Some(snap))
    }

    fn render_snapped(
        scene: &Scene,
        w: u32,
        h: u32,
        background: Color,
        force_snap: Option<Snap>,
    ) -> Result<Rgba8, CheckError> {
        Self::render_full(scene, w, h, background, force_snap, (1.0, None))
    }

    fn render_full(
        scene: &Scene,
        w: u32,
        h: u32,
        background: Color,
        force_snap: Option<Snap>,
        hint: (f64, Option<bool>),
    ) -> Result<Rgba8, CheckError> {
        if w == 0 || h == 0 {
            return Err(CheckError::Size { width: w, height: h });
        }
        let bg = premul(background);
        let mut t =
            Target { w, h, px: vec![bg; w as usize * h as usize], layers: Vec::new(), force_snap, hint };
        t.walk(scene.ops(), Affine::IDENTITY)?;
        // Unbalanced pushes close at the end of the scene (09 §7.2).
        while let Some(layer) = t.layers.pop() {
            match t.layers.last_mut() {
                Some(parent) => layer.pop_into(&mut parent.px),
                None => layer.pop_into(&mut t.px),
            }
        }
        let data = t.px.iter().flat_map(|p| p.map(quantize)).collect();
        Ok(Rgba8 { width: w, height: h, data })
    }
}
