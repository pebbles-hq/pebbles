//! COLR color glyphs in the golden model (V51 step 4b, 02 §7.7): skrifa walks the paint graph;
//! every callback is evaluated here in `f64` with the golden paint (gradients), compose (09 §4)
//! and coverage code. Written from the COLR spec and 02 §7.7, never from R29's code.

use kurbo::{Affine, BezPath, Rect};
use peniko::{Brush, Color, Compose, Extend, Fill, FontData, Gradient, Mix};
use skrifa::color::{Brush as ColrBrush, ColorPainter, ColorStop, CompositeMode, Transform};
use skrifa::raw::types::BoundingBox;
use skrifa::{GlyphId, MetadataProvider};

use super::compose::composite;
use super::coverage::fill_coverage;
use super::paint::Paint;

/// Premultiplied RGBA of COLR glyph `gid` over transparent on a `w × h` target. `t` maps font
/// units (y-up) to device px; palette 0; index `0xFFFF` = `foreground`. `None` when the font
/// has no COLR glyph `gid`.
pub fn colr_glyph_image(
    font: &FontData,
    gid: u16,
    coords: &[i16],
    t: Affine,
    foreground: Color,
    w: u32,
    h: u32,
) -> Option<Vec<[f64; 4]>> {
    let face = skrifa::FontRef::from_index(font.data.data(), font.index).ok()?;
    let glyph = face.color_glyphs().get(GlyphId::new(u32::from(gid)))?;
    let palette: Vec<Color> = face
        .color_palettes()
        .get(0)
        .map(|p| p.colors().iter().map(|c| Color::from_rgba8(c.red, c.green, c.blue, c.alpha)).collect())
        .unwrap_or_default();
    let n = w as usize * h as usize;
    let mut painter = Painter {
        font,
        t,
        palette,
        foreground,
        w,
        h,
        xf: vec![Affine::IDENTITY],
        clips: Vec::new(),
        layers: vec![(vec![[0.0; 4]; n], CompositeMode::SrcOver)],
    };
    let location: Vec<skrifa::instance::NormalizedCoord> =
        coords.iter().map(|&c| skrifa::instance::NormalizedCoord::from_bits(c)).collect();
    glyph.paint(skrifa::instance::LocationRef::new(&location), &mut painter).ok()?;
    painter.layers.pop().map(|l| l.0)
}

/// Font-unit outline of glyph `gid` (y-up), from swash at `size = upem`.
fn glyph_outline(font: &FontData, gid: u16) -> Option<BezPath> {
    let face = swash::FontRef::from_index(font.data.data(), font.index as usize)?;
    let upem = f32::from(face.metrics(&[]).units_per_em);
    let mut ctx = swash::scale::ScaleContext::new();
    let mut scaler = ctx.builder(face).size(upem).build();
    let outline = scaler.scale_outline(gid)?;
    let mut path = BezPath::new();
    let p = |q: swash::zeno::Point| kurbo::Point::new(f64::from(q.x), f64::from(q.y));
    let pts = outline.points();
    let mut i = 0;
    for verb in outline.verbs() {
        use swash::zeno::Verb;
        match verb {
            Verb::MoveTo => {
                path.move_to(p(pts[i]));
                i += 1;
            }
            Verb::LineTo => {
                path.line_to(p(pts[i]));
                i += 1;
            }
            Verb::QuadTo => {
                path.quad_to(p(pts[i]), p(pts[i + 1]));
                i += 2;
            }
            Verb::CurveTo => {
                path.curve_to(p(pts[i]), p(pts[i + 1]), p(pts[i + 2]));
                i += 3;
            }
            Verb::Close => path.close_path(),
        }
    }
    Some(path)
}

/// `CompositeMode` → peniko (09 §4 vocabulary).
pub fn blend_of(mode: CompositeMode) -> (Mix, Compose) {
    use CompositeMode as C;
    let compose = |c| (Mix::Normal, c);
    let mix = |m| (m, Compose::SrcOver);
    match mode {
        C::Clear => compose(Compose::Clear),
        C::Src => compose(Compose::Copy),
        C::Dest => compose(Compose::Dest),
        C::DestOver => compose(Compose::DestOver),
        C::SrcIn => compose(Compose::SrcIn),
        C::DestIn => compose(Compose::DestIn),
        C::SrcOut => compose(Compose::SrcOut),
        C::DestOut => compose(Compose::DestOut),
        C::SrcAtop => compose(Compose::SrcAtop),
        C::DestAtop => compose(Compose::DestAtop),
        C::Xor => compose(Compose::Xor),
        C::Plus => compose(Compose::Plus),
        C::Screen => mix(Mix::Screen),
        C::Overlay => mix(Mix::Overlay),
        C::Darken => mix(Mix::Darken),
        C::Lighten => mix(Mix::Lighten),
        C::ColorDodge => mix(Mix::ColorDodge),
        C::ColorBurn => mix(Mix::ColorBurn),
        C::HardLight => mix(Mix::HardLight),
        C::SoftLight => mix(Mix::SoftLight),
        C::Difference => mix(Mix::Difference),
        C::Exclusion => mix(Mix::Exclusion),
        C::Multiply => mix(Mix::Multiply),
        C::HslHue => mix(Mix::Hue),
        C::HslSaturation => mix(Mix::Saturation),
        C::HslColor => mix(Mix::Color),
        C::HslLuminosity => mix(Mix::Luminosity),
        // SrcOver and unknown modes.
        _ => compose(Compose::SrcOver),
    }
}

struct Painter<'a> {
    font: &'a FontData,
    t: Affine,
    palette: Vec<Color>,
    foreground: Color,
    w: u32,
    h: u32,
    /// Font-space transform stack (y-up); the device map is `t · top`.
    xf: Vec<Affine>,
    /// Clip coverage stack (each the product with its parent).
    clips: Vec<Vec<f64>>,
    /// Layer buffers with the mode they merge down with.
    layers: Vec<(Vec<[f64; 4]>, CompositeMode)>,
}

impl Painter<'_> {
    fn device(&self) -> Affine {
        self.t * self.xf.last().copied().unwrap_or(Affine::IDENTITY)
    }

    fn color(&self, index: u16, alpha: f32) -> Color {
        let c = if index == 0xFFFF {
            self.foreground
        } else {
            self.palette.get(usize::from(index)).copied().unwrap_or(Color::TRANSPARENT)
        };
        c.multiply_alpha(alpha)
    }

    fn stops(&self, stops: &[ColorStop]) -> Vec<peniko::ColorStop> {
        stops
            .iter()
            .map(|s| peniko::ColorStop::from((s.offset, self.color(s.palette_index, s.alpha))))
            .collect()
    }

    fn brush(&self, b: &ColrBrush<'_>) -> Brush {
        let ext = |e: skrifa::color::Extend| match e {
            skrifa::color::Extend::Repeat => Extend::Repeat,
            skrifa::color::Extend::Reflect => Extend::Reflect,
            _ => Extend::Pad,
        };
        let pt = |p: skrifa::raw::types::Point<f32>| (f64::from(p.x), f64::from(p.y));
        match b {
            ColrBrush::Solid { palette_index, alpha } => Brush::Solid(self.color(*palette_index, *alpha)),
            ColrBrush::LinearGradient { p0, p1, color_stops, extend } => Brush::Gradient(
                Gradient::new_linear(pt(*p0), pt(*p1))
                    .with_extend(ext(*extend))
                    .with_stops(self.stops(color_stops).as_slice()),
            ),
            ColrBrush::RadialGradient { c0, r0, c1, r1, color_stops, extend } => Brush::Gradient(
                Gradient::new_two_point_radial(pt(*c0), *r0, pt(*c1), *r1)
                    .with_extend(ext(*extend))
                    .with_stops(self.stops(color_stops).as_slice()),
            ),
            ColrBrush::SweepGradient { c0, start_angle, end_angle, color_stops, extend } => Brush::Gradient(
                Gradient::new_sweep(pt(*c0), start_angle.to_radians(), end_angle.to_radians())
                    .with_extend(ext(*extend))
                    .with_stops(self.stops(color_stops).as_slice()),
            ),
        }
    }

    fn push_clip_cov(&mut self, mut cov: Vec<f64>) {
        if let Some(parent) = self.clips.last() {
            cov.iter_mut().zip(parent).for_each(|(c, p)| *c *= p);
        }
        self.clips.push(cov);
    }
}

impl ColorPainter for Painter<'_> {
    fn push_transform(&mut self, t: Transform) {
        let m = Affine::new([t.xx, t.yx, t.xy, t.yy, t.dx, t.dy].map(f64::from));
        let top = self.xf.last().copied().unwrap_or(Affine::IDENTITY);
        self.xf.push(top * m);
    }

    fn pop_transform(&mut self) {
        if self.xf.len() > 1 {
            self.xf.pop();
        }
    }

    fn push_clip_glyph(&mut self, glyph_id: GlyphId) {
        let path = u16::try_from(glyph_id.to_u32())
            .ok()
            .and_then(|g| glyph_outline(self.font, g))
            .unwrap_or_default();
        let cov = fill_coverage(&path, self.device(), Fill::NonZero, self.w, self.h);
        self.push_clip_cov(cov);
    }

    fn push_clip_box(&mut self, b: BoundingBox<f32>) {
        let r = Rect::new(f64::from(b.x_min), f64::from(b.y_min), f64::from(b.x_max), f64::from(b.y_max));
        let cov =
            fill_coverage(&kurbo::Shape::to_path(&r, 0.0), self.device(), Fill::NonZero, self.w, self.h);
        self.push_clip_cov(cov);
    }

    fn pop_clip(&mut self) {
        self.clips.pop();
    }

    fn fill(&mut self, brush: ColrBrush<'_>) {
        let Ok(Some(paint)) = Paint::new(&self.brush(&brush), self.device(), 1.0) else { return };
        let w = self.w as usize;
        let clip = self.clips.last();
        let Some((buf, _)) = self.layers.last_mut() else { return };
        for (i, dst) in buf.iter_mut().enumerate() {
            let k = clip.map_or(1.0, |c| c[i]);
            if k == 0.0 {
                continue;
            }
            #[allow(clippy::cast_precision_loss)] // pixel indices < 2^24
            let s = paint.at((i % w) as f64 + 0.5, (i / w) as f64 + 0.5).map(|v| v * k);
            for ch in 0..4 {
                dst[ch] = s[ch] + dst[ch] * (1.0 - s[3]);
            }
        }
    }

    fn push_layer(&mut self, mode: CompositeMode) {
        let n = self.w as usize * self.h as usize;
        self.layers.push((vec![[0.0; 4]; n], mode));
    }

    fn pop_layer(&mut self) {
        if self.layers.len() < 2 {
            return;
        }
        let Some((src, mode)) = self.layers.pop() else { return };
        let (mix, compose) = blend_of(mode);
        if let Some((dst, _)) = self.layers.last_mut() {
            for (b, s) in dst.iter_mut().zip(&src) {
                *b = composite(*b, *s, mix, compose);
            }
        }
    }
}
