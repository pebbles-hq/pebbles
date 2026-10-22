//! Text decoration geometry (03 T43, Flutter `TextDecorationStyle`): every style becomes
//! ordinary fills / strokes through the `Painter` — cosmic-text's decoration rendering is never
//! used. `y` is the top edge of the decoration band and `t` its thickness (layout px).

use kurbo::{Affine, BezPath, Circle, Rect, Stroke};
use peniko::{Brush, Fill};

use crate::objects::TextDecorationStyle;
use crate::paint::Painter;

/// One piece of decoration geometry.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DecoShape {
    Rect(Rect),
    Circle(Circle),
    /// A centerline stroked at `width` and clipped to `clip`.
    Wave {
        path: BezPath,
        width: f64,
        clip: Rect,
    },
}

/// The normative geometry of `style` over `[x0, x1]` at band top `y`, thickness `t`.
pub(crate) fn geometry(style: TextDecorationStyle, x0: f64, x1: f64, y: f64, t: f64) -> Vec<DecoShape> {
    if x1 <= x0 || t <= 0.0 || !t.is_finite() {
        return Vec::new();
    }
    match style {
        TextDecorationStyle::Solid => vec![DecoShape::Rect(Rect::new(x0, y, x1, y + t))],
        TextDecorationStyle::Double => {
            vec![
                DecoShape::Rect(Rect::new(x0, y, x1, y + t)),
                DecoShape::Rect(Rect::new(x0, y + 2.0 * t, x1, y + 3.0 * t)),
            ]
        }
        TextDecorationStyle::Dotted => {
            let mut out = Vec::new();
            let mut cx = x0 + t / 2.0;
            while cx - t / 2.0 < x1 {
                out.push(DecoShape::Circle(Circle::new((cx, y + t / 2.0), t / 2.0)));
                cx += 2.0 * t;
            }
            out
        }
        TextDecorationStyle::Dashed => {
            let mut out = Vec::new();
            let mut x = x0;
            while x < x1 {
                out.push(DecoShape::Rect(Rect::new(x, y, (x + 3.0 * t).min(x1), y + t)));
                x += 5.0 * t;
            }
            out
        }
        TextDecorationStyle::Wavy => {
            // Quadratic zigzag: wavelength 4t, amplitude t, controls alternating ±t about the
            // band's center line.
            let mid = y + t / 2.0;
            let mut path = BezPath::new();
            path.move_to((x0, mid));
            let (mut x, mut up) = (x0, true);
            while x < x1 {
                let ctrl = if up { mid - 2.0 * t } else { mid + 2.0 * t };
                path.quad_to((x + t, ctrl), (x + 2.0 * t, mid));
                x += 2.0 * t;
                up = !up;
            }
            let clip = Rect::new(x0, mid - 2.0 * t, x1, mid + 2.0 * t);
            vec![DecoShape::Wave { path, width: t, clip }]
        }
    }
}

/// Paint `shapes` with `brush` (layout space already translated by `origin` into `transform`).
pub(crate) fn paint(scene: &mut Painter<'_>, transform: Affine, shapes: &[DecoShape], brush: &Brush) {
    for shape in shapes {
        match shape {
            DecoShape::Rect(r) => scene.fill(Fill::NonZero, transform, brush, None, r),
            DecoShape::Circle(c) => scene.fill(Fill::NonZero, transform, brush, None, c),
            DecoShape::Wave { path, width, clip } => {
                scene.push_layer(Fill::NonZero, peniko::Mix::Normal, 1.0, transform, clip);
                scene.stroke(&Stroke::new(*width), transform, brush, None, path);
                scene.pop_layer();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() <= 0.01
    }

    #[test]
    fn solid_is_one_band() {
        assert_eq!(
            geometry(TextDecorationStyle::Solid, 0.0, 20.0, 10.0, 1.0),
            [DecoShape::Rect(Rect::new(0.0, 10.0, 20.0, 11.0))]
        );
    }

    #[test]
    fn double_is_two_bands_two_t_apart() {
        let g = geometry(TextDecorationStyle::Double, 0.0, 20.0, 10.0, 1.5);
        assert_eq!(
            g,
            [
                DecoShape::Rect(Rect::new(0.0, 10.0, 20.0, 11.5)),
                DecoShape::Rect(Rect::new(0.0, 13.0, 20.0, 14.5))
            ]
        );
    }

    #[test]
    fn dotted_circles_every_two_t() {
        let g = geometry(TextDecorationStyle::Dotted, 0.0, 10.0, 0.0, 1.0);
        let centers: Vec<f64> = g
            .iter()
            .map(|s| match s {
                DecoShape::Circle(c) => c.center.x,
                _ => f64::NAN,
            })
            .collect();
        assert_eq!(centers, [0.5, 2.5, 4.5, 6.5, 8.5]);
        assert!(g.iter().all(|s| matches!(s, DecoShape::Circle(c) if near(c.radius, 0.5))));
    }

    #[test]
    fn dashed_three_t_every_five_t() {
        let g = geometry(TextDecorationStyle::Dashed, 0.0, 12.0, 0.0, 1.0);
        let xs: Vec<(f64, f64)> = g
            .iter()
            .map(|s| match s {
                DecoShape::Rect(r) => (r.x0, r.x1),
                _ => (f64::NAN, f64::NAN),
            })
            .collect();
        assert_eq!(xs, [(0.0, 3.0), (5.0, 8.0), (10.0, 12.0)]);
    }

    #[test]
    fn wavy_wavelength_and_amplitude() {
        let g = geometry(TextDecorationStyle::Wavy, 0.0, 8.0, 0.0, 1.0);
        let [DecoShape::Wave { path, width, clip }] = g.as_slice() else { panic!("{g:?}") };
        assert!(near(*width, 1.0));
        assert_eq!(*clip, Rect::new(0.0, -1.5, 8.0, 2.5));
        // 8 px at wavelength 4t = two periods = four quadratic arcs.
        assert_eq!(path.elements().iter().filter(|e| matches!(e, kurbo::PathEl::QuadTo(..))).count(), 4);
    }

    #[test]
    fn degenerate_input_draws_nothing() {
        assert!(geometry(TextDecorationStyle::Solid, 5.0, 5.0, 0.0, 1.0).is_empty());
        assert!(geometry(TextDecorationStyle::Dotted, 0.0, 5.0, 0.0, 0.0).is_empty());
    }
}
