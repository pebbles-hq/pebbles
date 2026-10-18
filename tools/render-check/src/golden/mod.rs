//! The golden model (05 §6.7a, DR-21): an independent CPU implementation of the raster spec,
//! written for exactness, not speed. Shares no rasterization code with pebbles-gpu.

pub mod coverage;
pub mod shapes;
pub mod stroke;

use kurbo::Affine;
use pebbles_render::paint::Scene;
use pebbles_render::paint::display_list::{Op, PixelSnap};
use peniko::{BlendMode, Brush, Color, Compose, Mix};

use crate::CheckError;
use crate::metric::Rgba8;
use shapes::{Snap, fill_geometry, stroke_geometry};

/// The golden renderer.
pub struct Golden;

struct Target {
    w: u32,
    h: u32,
    /// Premultiplied, sRGB-encoded, `f64` (02 §3).
    px: Vec<[f64; 4]>,
    /// Clip masks of the open layers (each the product with its parent).
    clips: Vec<Vec<f64>>,
    /// `Some` forces a policy for every op; `None` honors each op's `PixelSnap` (02 §5.3).
    force_snap: Option<Snap>,
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

    /// Source-over `src · cov · clip` into every pixel.
    fn composite(&mut self, src: [f64; 4], cov: &[f64]) {
        let clip = self.clips.last();
        for (i, (dst, &c)) in self.px.iter_mut().zip(cov).enumerate() {
            let k = c * clip.map_or(1.0, |m| m[i]);
            if k == 0.0 {
                continue;
            }
            let inv = 1.0 - src[3] * k;
            for ch in 0..4 {
                dst[ch] = src[ch] * k + dst[ch] * inv;
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
            Op::Fill { style, transform, brush, shape, snap, .. } => {
                let src = solid(brush)?;
                let cov = fill_geometry(shape, outer * *transform, *style, self.snap(*snap), self.w, self.h);
                self.composite(src, &cov);
            }
            Op::Stroke { style, transform, brush, shape, snap, .. } => {
                let src = solid(brush)?;
                let cov = stroke_geometry(shape, style, outer * *transform, self.snap(*snap), self.w, self.h);
                self.composite(src, &cov);
            }
            Op::PushLayer { clip_fill, blend, alpha, transform, clip, .. } => {
                if *blend != BlendMode::new(Mix::Normal, Compose::SrcOver) {
                    return Err(CheckError::GoldenUnsupported("layer-blend"));
                }
                if *alpha != 1.0 {
                    return Err(CheckError::GoldenUnsupported("layer-alpha"));
                }
                let mut mask = fill_geometry(clip, outer * *transform, *clip_fill, Snap::Off, self.w, self.h);
                if let Some(parent) = self.clips.last() {
                    mask.iter_mut().zip(parent).for_each(|(m, p)| *m *= p);
                }
                self.clips.push(mask);
            }
            Op::PopLayer => {
                self.clips.pop();
            }
            Op::Fragment { transform, ops, .. } => {
                self.walk(ops, outer * transform.unwrap_or(Affine::IDENTITY))?;
            }
            Op::Image { .. } => return Err(CheckError::GoldenUnsupported("image")),
            Op::BlurredRect { .. } => return Err(CheckError::GoldenUnsupported("blurred-rect")),
            Op::Glyphs { .. } => return Err(CheckError::GoldenUnsupported("glyphs")),
            _ => return Err(CheckError::GoldenUnsupported("unknown-op")),
        }
        Ok(())
    }
}

fn solid(brush: &Brush) -> Result<[f64; 4], CheckError> {
    match brush {
        Brush::Solid(c) => Ok(premul(*c)),
        Brush::Gradient(_) => Err(CheckError::GoldenUnsupported("gradient")),
        Brush::Image(_) => Err(CheckError::GoldenUnsupported("image-brush")),
    }
}

impl Golden {
    /// Render `scene` (device px) over `background`, honoring each op's `PixelSnap`.
    pub fn render(scene: &Scene, w: u32, h: u32, background: Color) -> Result<Rgba8, CheckError> {
        Self::render_snapped(scene, w, h, background, None)
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
        if w == 0 || h == 0 {
            return Err(CheckError::Size { width: w, height: h });
        }
        let bg = premul(background);
        let mut t = Target { w, h, px: vec![bg; w as usize * h as usize], clips: Vec::new(), force_snap };
        t.walk(scene.ops(), Affine::IDENTITY)?;
        let data = t.px.iter().flat_map(|p| p.map(quantize)).collect();
        Ok(Rgba8 { width: w, height: h, data })
    }
}
