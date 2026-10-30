//! Glyph runs in the golden model (05 §6.7a, V46 step 2): swash **outlines** at the physical
//! size, placed by the 02 §7.4 rules (pen x to the nearest quarter pixel, baseline y to a whole
//! pixel), filled with the golden coverage (non-zero). Independent of swash's and zeno's mask
//! rasterizers.

use kurbo::{Affine, BezPath, Point};
use pebbles_render::paint::Glyph;
use peniko::{Fill, FontData};
use swash::scale::ScaleContext;
use swash::zeno::Verb;

use super::coverage::fill_coverage;

/// The physical raster size of a run drawn with `font_size` under `t` (02 §7: the x-axis scale).
fn size_px(font_size: f32, t: Affine) -> f64 {
    let [a, b, ..] = t.as_coeffs();
    f64::from(font_size) * a.hypot(b)
}

/// The device placement of a glyph whose pen maps to `pen`: (x snapped to ¼ px, y to 1 px).
fn placement(pen: Point) -> Point {
    Point::new((pen.x * 4.0).round() / 4.0, pen.y.round())
}

/// Coverage of glyph `id` of `font` at `size` px (`hint` per 02 §7.8) with its pen at device
/// `pen` (already snapped), over a `w × h` target.
pub fn glyph_coverage(
    font: &FontData,
    id: u16,
    size: f32,
    hint: bool,
    pen: Point,
    w: u32,
    h: u32,
) -> Option<Vec<f64>> {
    let face = swash::FontRef::from_index(font.data.data(), font.index as usize)?;
    let mut ctx = ScaleContext::new();
    let mut scaler = ctx.builder(face).size(size).hint(hint).build();
    let outline = scaler.scale_outline(id)?;
    Some(fill_coverage(&outline_path(&outline, pen), Affine::IDENTITY, Fill::NonZero, w, h))
}

/// The kurbo path of a y-up, pen-relative swash outline placed at device `pen`.
fn outline_path(outline: &swash::scale::outline::Outline, pen: Point) -> BezPath {
    let p = |q: swash::zeno::Point| Point::new(pen.x + f64::from(q.x), pen.y - f64::from(q.y));
    let pts = outline.points();
    let mut path = BezPath::new();
    let mut i = 0;
    for verb in outline.verbs() {
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
    path
}

/// The union of a run's glyph outlines in run space (each at its pen, under `G · FLIP_Y`) — the
/// shape `BlurredGlyphs` blurs (02 R42).
pub fn run_union(
    font: &FontData,
    font_size: f32,
    coords: &[i16],
    g: Option<Affine>,
    glyphs: &[Glyph],
) -> BezPath {
    let mut union = BezPath::new();
    let Some(face) = swash::FontRef::from_index(font.data.data(), font.index as usize) else { return union };
    let mut ctx = ScaleContext::new();
    let mut scaler =
        ctx.builder(face).size(font_size).hint(false).normalized_coords(coords.iter().copied()).build();
    for gl in glyphs {
        let Ok(id) = u16::try_from(gl.id) else { continue };
        let Some(outline) = scaler.scale_outline(id) else { continue };
        let t = Affine::translate((f64::from(gl.x), f64::from(gl.y))) * g.unwrap_or(Affine::IDENTITY);
        union.extend((t * outline_path(&outline, Point::ZERO)).elements().iter().copied());
    }
    union
}

/// Device ppem above which atlas-route runs use outlines (10 §8.2 `MAX_ATLAS_GLYPH_PX`).
const MAX_ATLAS_GLYPH_PX: f64 = 256.0;

/// 02 §7.5: `Some(a)` (the uniform scale) for the atlas route, `None` for the outline route.
fn atlas_scale(m: Affine, glyph_transform: Option<Affine>, font_size: f32) -> Option<f64> {
    if glyph_transform.is_some_and(|g| g != Affine::IDENTITY) {
        return None;
    }
    let [a, b, c, d, ..] = m.as_coeffs();
    let s = a.abs().max(d.abs());
    let uniform =
        b.abs() <= 1e-6 * s && c.abs() <= 1e-6 * s && a > 0.0 && d > 0.0 && (a - d).abs() <= 1e-6 * a;
    (uniform && f64::from(font_size) * a <= MAX_ATLAS_GLYPH_PX).then_some(a)
}

/// One glyph's contribution: coverage painted by the run's brush, or a premultiplied color
/// image over transparent (COLR and bitmap glyphs, 02 §7.7) drawn at the run's alpha.
pub enum GlyphOut {
    Coverage(Vec<f64>),
    Color(Vec<[f64; 4]>),
}

/// The glyphs of one run over a `w × h` target, in glyph order (each glyph is returned
/// separately: glyph quads draw source-over one after another). Per glyph, the 02 §7.7 source
/// order: COLR → bitmap strike → outline. `foreground` = the run color (COLR index `0xFFFF`).
#[allow(clippy::too_many_arguments)]
pub fn run_coverage(
    font: &FontData,
    font_size: f32,
    coords: &[i16],
    t: Affine,
    glyph_transform: Option<Affine>,
    glyphs: &[Glyph],
    w: u32,
    h: u32,
    hint: (f64, Option<bool>),
    foreground: peniko::Color,
    force_outline: bool,
    solid: bool,
) -> Vec<GlyphOut> {
    let Some(face) = swash::FontRef::from_index(font.data.data(), font.index as usize) else {
        return Vec::new();
    };
    // 09 §5.6: a gradient-painted run always takes the outline route.
    let atlas = if force_outline { None } else { atlas_scale(t, glyph_transform, font_size) };
    // Outline route (§7.6): the outline at the run's own size, mapped by the full transform.
    let size = if atlas.is_some() { size_px(font_size, t) } else { f64::from(font_size) };
    if !(size.is_finite() && size > 0.0) {
        return Vec::new();
    }
    // 02 §7.8: the atlas route hints below 2× and up to 48 ppem; the outline route never.
    let hinted = atlas.is_some() && hint.1.unwrap_or(hint.0 < 2.0 && size <= 48.0);
    let upem = f64::from(face.metrics(&[]).units_per_em.max(1));
    let g_xf = glyph_transform.unwrap_or(Affine::IDENTITY);
    let mut ctx = ScaleContext::new();
    let mut out = Vec::new();
    for g in glyphs {
        let Ok(id) = u16::try_from(g.id) else { continue };
        let pen = Affine::translate((f64::from(g.x), f64::from(g.y)));
        // COLR: font units (y-up) → device under M · translate(pen) · G · scale(size/upem) · FLIP_Y.
        let k = f64::from(font_size) / upem;
        let colr_t = t * pen * g_xf * Affine::scale_non_uniform(k, -k);
        if let Some(img) = super::colr::colr_glyph_image(font, id, coords, colr_t, foreground, w, h) {
            out.push(GlyphOut::Color(img));
            continue;
        }
        if let Some(img) = bitmap_glyph(
            face,
            &mut ctx,
            id,
            atlas.is_some(),
            size,
            f64::from(font_size),
            t * pen * g_xf,
            w,
            h,
        ) {
            out.push(GlyphOut::Color(img));
            continue;
        }
        let mut scaler = ctx
            .builder(face)
            .size(size as f32)
            .hint(hinted)
            .normalized_coords(coords.iter().copied())
            .build();
        let Some(outline) = scaler.scale_outline(id) else { continue };
        let path = if atlas.is_some() {
            outline_path(&outline, placement(t * Point::new(f64::from(g.x), f64::from(g.y))))
        } else {
            // device = M · ((g.x, g.y) + G · FLIP_Y · outline_point)
            (t * pen * g_xf) * outline_path(&outline, Point::ZERO)
        };
        let mut cov = fill_coverage(&path, Affine::IDENTITY, Fill::NonZero, w, h);
        if atlas.is_some() && solid {
            text_gamma(&mut cov, foreground);
        }
        out.push(GlyphOut::Coverage(cov));
    }
    out
}

/// Atlas-route mask glyphs: the 02 text coverage curve (GPUI `apply_contrast_and_gamma_correction`,
/// from microsoft/terminal dwrite.hlsl) at the renderer defaults — gamma 1.8, grayscale
/// enhanced contrast 1.0 — applied to the 8-bit atlas coverage. Written from the formula, not
/// shared with pebbles-gpu.
fn text_gamma(cov: &mut [f64], fg: peniko::Color) {
    // DirectWrite gamma-incorrect-target ratios for gamma 1.8, normalised to 0..=1 coverage.
    let n13 = 65536.0 / (255.0 * 255.0);
    let n24 = 256.0 / 255.0;
    let g = [0.1469 * n13, -0.8911 * n24, 1.4644 * n13, -0.3234 * n24];
    let [r, gr, b, _] = fg.components.map(f64::from);
    let lum = 0.30 * r + 0.59 * gr + 0.11 * b;
    let k = (4.0 * (0.75 - lum)).clamp(0.0, 1.0);
    for c in cov.iter_mut().filter(|c| **c > 0.0) {
        let a8 = (c.clamp(0.0, 1.0) * 255.0).round() / 255.0;
        let a = a8 * (k + 1.0) / (a8 * k + 1.0);
        let corr = (g[0] * lum + g[1]) * a + (g[2] * lum + g[3]);
        *c = (a + a * (1.0 - a) * corr).clamp(0.0, 1.0);
    }
}

/// Exact box filter of premultiplied texels by `s` (`0 < s < 1`, 02 §7.7 / R23): output pixel
/// `(x, y)` = the area-weighted mean of the source texels under `[x/s, (x+1)/s) × [y/s, (y+1)/s)`
/// clipped to the source. Written from the R23 spec text.
pub fn box_downscale(src: &[[f64; 4]], w: u32, h: u32, s: f64) -> (Vec<[f64; 4]>, u32, u32) {
    let out_dim = |n: u32| -> u32 {
        let v = (f64::from(n) * s - 1e-4).ceil().max(1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // ≤ n
        let v = v as u32;
        v
    };
    let (ow, oh) = (out_dim(w), out_dim(h));
    let mut out = vec![[0.0; 4]; ow as usize * oh as usize];
    for oy in 0..oh {
        let (y0, y1) = (f64::from(oy) / s, (f64::from(oy + 1) / s).min(f64::from(h)));
        for ox in 0..ow {
            let (x0, x1) = (f64::from(ox) / s, (f64::from(ox + 1) / s).min(f64::from(w)));
            let (mut acc, mut area) = ([0.0; 4], 0.0);
            for sy in 0..h {
                let wy = (f64::from(sy + 1).min(y1) - f64::from(sy).max(y0)).max(0.0);
                if wy == 0.0 {
                    continue;
                }
                for sx in 0..w {
                    let wx = (f64::from(sx + 1).min(x1) - f64::from(sx).max(x0)).max(0.0);
                    if wx == 0.0 {
                        continue;
                    }
                    let t = src[(sy * w + sx) as usize];
                    for k in 0..4 {
                        acc[k] += t[k] * wx * wy;
                    }
                    area += wx * wy;
                }
            }
            if area > 0.0 {
                out[(oy * ow + ox) as usize] = acc.map(|v| v / area);
            }
        }
    }
    (out, ow, oh)
}

/// A bitmap-strike color glyph (V51 step 4, 02 §7.7, R23): decoded at the strike's own ppem,
/// premultiplied, then (atlas route) box-downscaled to the device ppem or drawn as a scaled quad,
/// or (outline route) the largest strike as a transformed quad under `m = M · translate(pen) · G`.
#[allow(clippy::too_many_arguments)]
fn bitmap_glyph(
    face: swash::FontRef<'_>,
    ctx: &mut ScaleContext,
    id: u16,
    atlas_route: bool,
    ppem: f64,
    font_size: f64,
    m: Affine,
    w: u32,
    h: u32,
) -> Option<Vec<[f64; 4]>> {
    use swash::scale::{Render, Source, StrikeWith};
    let strikes = face.color_strikes();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // ppem ≤ MAX_ATLAS_GLYPH_PX
    let strike = if atlas_route {
        strikes.find_by_nearest_ppem(ppem.ceil().clamp(1.0, 65535.0) as u16, id)?
    } else {
        strikes.find_by_largest_ppem(id)?
    };
    let sp = f64::from(strike.ppem());
    #[allow(clippy::cast_possible_truncation)] // strike ppem ≤ 65535
    let mut scaler = ctx.builder(face).size(sp as f32).build();
    let img = Render::new(&[Source::ColorBitmap(StrikeWith::ExactSize)])
        .format(swash::zeno::Format::Alpha)
        .render(&mut scaler, id)?;
    let pl = img.placement;
    let texels: Vec<[f64; 4]> = img
        .data
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            let a = f64::from(p[3]) / 255.0;
            [f64::from(p[0]) / 255.0 * a, f64::from(p[1]) / 255.0 * a, f64::from(p[2]) / 255.0 * a, a]
        })
        .collect();
    let (left, top) = (f64::from(pl.left), f64::from(pl.top));
    // (texels, width, height, texel → device): the quad is the texel rect `(0, 0, w, h)`.
    let (texels, tw, th, texel_to_device) = if atlas_route {
        let s = ppem / sp;
        let pen = m * Point::ZERO;
        // 02 §7.4: x to the quarter pixel, whose integer part places the bitmap; y to a whole pixel.
        let (ix, iy) = (((pen.x * 4.0).round() / 4.0).floor(), pen.y.round());
        let (x0, y0) = (ix + (left * s).round(), iy - (top * s).round());
        if s < 1.0 {
            // Downscaled in premultiplied space and drawn 1:1.
            let (t2, tw, th) = box_downscale(&texels, pl.width, pl.height, s);
            (t2, tw, th, Affine::translate((x0, y0)))
        } else {
            // A smaller strike: uploaded as is, drawn as a quad of rounded size (R23 1b).
            let (qw, qh) = ((f64::from(pl.width) * s).round(), (f64::from(pl.height) * s).round());
            let map = Affine::translate((x0, y0))
                * Affine::scale_non_uniform(qw / f64::from(pl.width), qh / f64::from(pl.height));
            (texels, pl.width, pl.height, map)
        }
    } else {
        // The native strike under M · translate(pen) · G · scale(font_size / strike ppem).
        let map = m * Affine::scale(font_size / sp) * Affine::translate((left, -top));
        (texels, pl.width, pl.height, map)
    };
    let quad = kurbo::Rect::new(0.0, 0.0, f64::from(tw), f64::from(th));
    let tex = super::paint::Texels { w: tw, h: th, px: texels };
    let paint = super::paint::Paint::Image {
        inv: texel_to_device.inverse(),
        img: tex,
        x_extend: peniko::Extend::Pad,
        y_extend: peniko::Extend::Pad,
        nearest: false,
        alpha: 1.0,
    };
    let cov = fill_coverage(&kurbo::Shape::to_path(&quad, 0.0), texel_to_device, Fill::NonZero, w, h);
    let wu = w as usize;
    Some(
        cov.iter()
            .enumerate()
            .map(|(i, &k)| {
                if k == 0.0 {
                    return [0.0; 4];
                }
                #[allow(clippy::cast_precision_loss)] // pixel indices < 2^24
                paint.at((i % wu) as f64 + 0.5, (i / wu) as f64 + 0.5).map(|v| v * k)
            })
            .collect(),
    )
}
