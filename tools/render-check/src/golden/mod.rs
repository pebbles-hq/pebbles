//! The golden model (05 §6.7a, DR-21): an independent CPU implementation of the raster spec,
//! written for exactness, not speed. Shares no rasterization code with pebbles-gpu.
//!
//! Op coverage (02 §2 matrix, V45 AC2): `Fill`/`Stroke` with solid, gradient (every kind and
//! extend, DR-05 interpolation) and image brushes; `PushLayer`/`PopLayer` with every clip
//! geometry, group alpha and every `Mix` × `Compose`; `Image`; `BlurredRect`; `Glyphs` (outline
//! and atlas routes, hinting, any brush); `Fragment`. Added by V51: mip chains (R35), nine-slice
//! (R36), dynamic images (R37), blurred masks (R42), bitmap-strike glyph resampling (R23) and
//! COLR glyphs (R29); by V52: effects, backdrop filters, projected layers, meshes, sprites.

pub mod blur;
pub mod colr;
pub mod compose;
pub mod coverage;
pub mod layers;
pub mod nine;
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
            let mut s = src.map(|v| v * k);
            if paint.dithers() {
                s = paint::dither(s, i % w, i / w);
            }
            let inv = 1.0 - s[3];
            for ch in 0..4 {
                dst[ch] = s[ch] + dst[ch] * inv;
            }
        }
    }

    /// 02 R42: `path`'s coverage (8-bit, on the floored device bbox padded by ⌈3σ⌉ — the
    /// renderer's mask region), blurred with the 09 §10.2 kernel, combined per `style`, painted.
    fn blurred(
        &mut self,
        path: &kurbo::BezPath,
        t: Affine,
        rule: peniko::Fill,
        color: Color,
        sigma: f32,
        style: pebbles_render::paint::display_list::BlurStyle,
    ) {
        use kurbo::Shape as _;
        let [a, b, c, d, ..] = t.as_coeffs();
        let s1 = a * a + b * b + c * c + d * d;
        let det = (a * d - b * c).abs();
        let smax = ((s1 + (s1 * s1 - 4.0 * det * det).max(0.0).sqrt()) / 2.0).sqrt();
        let sd = f64::from(sigma.max(0.0)) * smax;
        let r = (3.0 * sd).ceil();
        let bb = t.transform_rect_bbox(path.bounding_box());
        if !bb.is_finite() {
            return;
        }
        let (x0, y0) = (bb.x0.floor() - r, bb.y0.floor() - r);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded masks
        let (pw, ph) = (((bb.x1.ceil() + r) - x0) as usize, ((bb.y1.ceil() + r) - y0) as usize);
        let cov =
            coverage::fill_coverage(path, Affine::translate((-x0, -y0)) * t, rule, pw as u32, ph as u32);
        let q: Vec<f64> = cov.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() / 255.0).collect();
        let bl = blur::blur(&q, pw, ph, sd, true);
        let st = match style {
            pebbles_render::paint::display_list::BlurStyle::Solid => blur::BlurStyle::Solid,
            pebbles_render::paint::display_list::BlurStyle::Outer => blur::BlurStyle::Outer,
            pebbles_render::paint::display_list::BlurStyle::Inner => blur::BlurStyle::Inner,
            _ => blur::BlurStyle::Normal,
        };
        let src = premul(color);
        let (w, h) = (self.w as i64, self.h as i64);
        let buf = match self.layers.last_mut() {
            Some(l) => &mut l.px,
            None => &mut self.px,
        };
        #[allow(clippy::cast_possible_truncation)] // pixel coordinates
        let (ox, oy) = (x0 as i64, y0 as i64);
        for yy in 0..ph {
            for xx in 0..pw {
                let (dx, dy) = (ox + xx as i64, oy + yy as i64);
                if dx < 0 || dy < 0 || dx >= w || dy >= h {
                    continue;
                }
                let i = yy * pw + xx;
                let k = blur::style(st, q[i], bl[i]);
                if k == 0.0 {
                    continue;
                }
                let dst = &mut buf[(dy * w + dx) as usize];
                for ch in 0..4 {
                    dst[ch] = src[ch] * k + dst[ch] * (1.0 - src[3] * k);
                }
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
            Op::BlurredFill { style, transform, color, shape, sigma, blur_style, .. } => {
                let path = shape.to_path(0.01).into_owned();
                self.blurred(&path, outer * *transform, *style, *color, *sigma, *blur_style);
            }
            Op::BlurredGlyphs {
                font,
                font_size,
                normalized_coords,
                transform,
                glyph_transform,
                glyphs,
                color,
                sigma,
                ..
            } => {
                let path = text::run_union(font, *font_size, normalized_coords, *glyph_transform, glyphs);
                let s = pebbles_render::paint::display_list::BlurStyle::Normal;
                self.blurred(&path, outer * *transform, peniko::Fill::NonZero, *color, *sigma, s);
            }
            Op::DynamicImage { frame, quality, alpha, transform, .. } => {
                // 02 R37: drawn exactly like `Image` (premultiplied pixels, never mipmapped).
                let data = peniko::ImageData {
                    data: frame.data.clone(),
                    format: peniko::ImageFormat::Rgba8,
                    alpha_type: peniko::ImageAlphaType::AlphaPremultiplied,
                    width: frame.width,
                    height: frame.height,
                };
                let q = if *quality == peniko::ImageQuality::Low {
                    peniko::ImageQuality::Low
                } else {
                    peniko::ImageQuality::Medium
                };
                let t = outer * *transform;
                let brush = peniko::ImageBrush::new(data).with_quality(q).with_alpha(*alpha);
                // Medium is bilinear on the single level (paint.rs only mips when LOD > 0 on
                // static images; dynamic images are never mipmapped).
                let Some(paint) = Paint::new(&peniko::Brush::Image(brush), t, 1.0)? else { return Ok(()) };
                let paint = match paint {
                    Paint::Mipped { inv, levels, x_extend, y_extend, alpha, .. } => Paint::Image {
                        inv,
                        img: levels.into_iter().next().unwrap_or(paint::Texels {
                            w: 0,
                            h: 0,
                            px: Vec::new(),
                        }),
                        x_extend,
                        y_extend,
                        nearest: false,
                        alpha,
                    },
                    other => other,
                };
                let r = kurbo::Rect::new(0.0, 0.0, f64::from(frame.width), f64::from(frame.height));
                let shape = pebbles_render::paint::display_list::Geometry::Rect(r);
                let cov = fill_geometry(&shape, t, peniko::Fill::NonZero, Snap::Auto, self.w, self.h);
                self.composite(&paint, &cov);
            }
            Op::ImageNine { image, center, dst, transform, .. } => {
                // 02 R36: per slice, the destination rect's exact coverage, sampled bilinearly
                // from the slice with UVs inset by half a texel (no bleeding across slices).
                let t = outer * *transform;
                let img = paint::Texels::of(&image.image);
                let (w, h) = (f64::from(img.w), f64::from(img.h));
                let alpha = f64::from(image.sampler.alpha).clamp(0.0, 1.0);
                if t.determinant().abs() < 1e-12 || alpha <= 0.0 {
                    return Ok(());
                }
                let inv = t.inverse();
                for (s, d) in nine::nine_slice_rects(w, h, *center, dst.abs()) {
                    let shape = pebbles_render::paint::display_list::Geometry::Rect(d);
                    let cov = fill_geometry(&shape, t, peniko::Fill::NonZero, Snap::Off, self.w, self.h);
                    let hx = 0.5_f64.min(0.5 * s.width());
                    let hy = 0.5_f64.min(0.5 * s.height());
                    let wu = self.w as usize;
                    let px: Vec<[f64; 4]> = cov
                        .iter()
                        .enumerate()
                        .map(|(i, &k)| {
                            if k == 0.0 {
                                return [0.0; 4];
                            }
                            #[allow(clippy::cast_precision_loss)] // pixel indices < 2^24
                            let q = inv * kurbo::Point::new((i % wu) as f64 + 0.5, (i / wu) as f64 + 0.5);
                            let (tx, ty) = ((q.x - d.x0) / d.width(), (q.y - d.y0) / d.height());
                            let u = s.x0 + hx + tx * (s.width() - 2.0 * hx);
                            let v = s.y0 + hy + ty * (s.height() - 2.0 * hy);
                            let c = img.sample(u, v, peniko::Extend::Pad, peniko::Extend::Pad, false);
                            c.map(|x| x * k * alpha)
                        })
                        .collect();
                    self.composite_image(&px, 1.0);
                }
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
                let outline = matches!(brush, peniko::Brush::Gradient(_));
                for out in text::run_coverage(font, *font_size, coords, t, g, glyphs, w, h, hint, fg, outline)
                {
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
        // The target holds premultiplied values; quantize them like the GPU's 8-bit target, then
        // unpremultiply exactly as the candidate readback does (non-opaque results, e.g. Clear).
        let mut data: Vec<u8> = t.px.iter().flat_map(|p| p.map(quantize)).collect();
        crate::readback::unpremultiply_in_place(&mut data);
        Ok(Rgba8 { width: w, height: h, data })
    }
}
