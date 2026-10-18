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

/// Coverage of one run over a `w × h` target, accumulated as the union of the glyph fills
/// (glyph quads draw source-over one after another, so each glyph is returned separately).
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
) -> Vec<Vec<f64>> {
    let Some(face) = swash::FontRef::from_index(font.data.data(), font.index as usize) else {
        return Vec::new();
    };
    let atlas = atlas_scale(t, glyph_transform, font_size);
    // Outline route (§7.6): the outline at the run's own size, mapped by the full transform.
    let size = if atlas.is_some() { size_px(font_size, t) } else { f64::from(font_size) };
    if !(size.is_finite() && size > 0.0) {
        return Vec::new();
    }
    let mut ctx = ScaleContext::new();
    let mut scaler =
        ctx.builder(face).size(size as f32).hint(false).normalized_coords(coords.iter().copied()).build();
    let mut out = Vec::new();
    for g in glyphs {
        let Ok(id) = u16::try_from(g.id) else { continue };
        let Some(outline) = scaler.scale_outline(id) else { continue };
        let path = if atlas.is_some() {
            outline_path(&outline, placement(t * Point::new(f64::from(g.x), f64::from(g.y))))
        } else {
            // device = M · ((g.x, g.y) + G · FLIP_Y · outline_point)
            let m = t
                * Affine::translate((f64::from(g.x), f64::from(g.y)))
                * glyph_transform.unwrap_or(Affine::IDENTITY);
            m * outline_path(&outline, Point::ZERO)
        };
        out.push(fill_coverage(&path, Affine::IDENTITY, Fill::NonZero, w, h));
    }
    out
}
