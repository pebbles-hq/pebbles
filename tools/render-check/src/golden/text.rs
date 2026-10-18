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

/// Coverage of one run over a `w × h` target, accumulated as the union of the glyph fills
/// (glyph quads draw source-over one after another, so each glyph is returned separately).
pub fn run_coverage(
    font: &FontData,
    font_size: f32,
    coords: &[i16],
    t: Affine,
    glyphs: &[Glyph],
    w: u32,
    h: u32,
) -> Vec<Vec<f64>> {
    let Some(face) = swash::FontRef::from_index(font.data.data(), font.index as usize) else {
        return Vec::new();
    };
    let size = size_px(font_size, t);
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
        let origin = placement(t * Point::new(f64::from(g.x), f64::from(g.y)));
        // swash outlines are y-up in px from the pen.
        let p = |q: swash::zeno::Point| Point::new(origin.x + f64::from(q.x), origin.y - f64::from(q.y));
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
        out.push(fill_coverage(&path, Affine::IDENTITY, Fill::NonZero, w, h));
    }
    out
}
