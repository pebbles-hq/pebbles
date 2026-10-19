//! Projective (perspective) transforms (09 §10.7): a row-major 3×3 homography `H` mapping
//! layer-local `(x, y, 1)` to `(x′, y′, w′)`, then dividing by `w′`. Content is planar at z = 0,
//! like Flutter's `Transform(transform: Matrix4)` with a perspective entry.

use kurbo::{Affine, Point};

/// A 3×3 homography, row-major.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projective(pub [f64; 9]);

impl Projective {
    /// From Flutter's column-major `Matrix4` storage, keeping the x, y and w rows and columns:
    /// `H = [[m0, m4, m12], [m1, m5, m13], [m3, m7, m15]]`.
    pub fn from_matrix4(m: [f64; 16]) -> Self {
        Self([m[0], m[4], m[12], m[1], m[5], m[13], m[3], m[7], m[15]])
    }

    pub fn from_affine(a: Affine) -> Self {
        let [a0, b, c, d, e, f] = a.as_coeffs();
        Self([a0, c, e, b, d, f, 0.0, 0.0, 1.0])
    }

    /// No perspective row (`[0, 0, 1]` up to scale).
    pub fn is_affine(&self) -> bool {
        let h = &self.0;
        h[6].abs() < 1e-12 && h[7].abs() < 1e-12 && (h[8] - 1.0).abs() < 1e-12
    }

    /// The affine part (exact when [`is_affine`](Self::is_affine)).
    pub fn to_affine(&self) -> Affine {
        let h = &self.0;
        let k = if h[8].abs() > 1e-12 { 1.0 / h[8] } else { 1.0 };
        Affine::new([h[0] * k, h[3] * k, h[1] * k, h[4] * k, h[2] * k, h[5] * k])
    }

    /// `self · other`.
    pub fn then(&self, other: &Projective) -> Projective {
        let (a, b) = (&other.0, &self.0);
        Projective(std::array::from_fn(|i| {
            let (r, c) = (i / 3, i % 3);
            (0..3).map(|k| a[r * 3 + k] * b[k * 3 + c]).sum()
        }))
    }

    pub fn inverse(&self) -> Option<Projective> {
        let m = &self.0;
        let det = m[0] * (m[4] * m[8] - m[5] * m[7]) - m[1] * (m[3] * m[8] - m[5] * m[6])
            + m[2] * (m[3] * m[7] - m[4] * m[6]);
        if !det.is_finite() || det.abs() < 1e-15 {
            return None;
        }
        let inv = [
            m[4] * m[8] - m[5] * m[7],
            m[2] * m[7] - m[1] * m[8],
            m[1] * m[5] - m[2] * m[4],
            m[5] * m[6] - m[3] * m[8],
            m[0] * m[8] - m[2] * m[6],
            m[2] * m[3] - m[0] * m[5],
            m[3] * m[7] - m[4] * m[6],
            m[1] * m[6] - m[0] * m[7],
            m[0] * m[4] - m[1] * m[3],
        ];
        Some(Projective(inv.map(|v| v / det)))
    }

    /// `(x′, y′, w′)` of local `p`.
    pub fn map_homogeneous(&self, p: Point) -> (f64, f64, f64) {
        let h = &self.0;
        (h[0] * p.x + h[1] * p.y + h[2], h[3] * p.x + h[4] * p.y + h[5], h[6] * p.x + h[7] * p.y + h[8])
    }

    /// The image of `p`, or `None` behind the camera (`w′ ≤ 0`).
    pub fn map_point(&self, p: Point) -> Option<Point> {
        let (x, y, w) = self.map_homogeneous(p);
        (w > 0.0 && w.is_finite()).then(|| Point::new(x / w, y / w))
    }

    /// The larger singular value of `∂(x′/w′, y′/w′)/∂(x, y)` at `p` (`None` behind the camera).
    pub fn local_scale(&self, p: Point) -> Option<f64> {
        let (x, y, w) = self.map_homogeneous(p);
        if w <= 0.0 {
            return None;
        }
        let h = &self.0;
        let (a, b) = ((h[0] * w - x * h[6]) / (w * w), (h[1] * w - x * h[7]) / (w * w));
        let (c, d) = ((h[3] * w - y * h[6]) / (w * w), (h[4] * w - y * h[7]) / (w * w));
        let t = a * a + b * b + c * c + d * d;
        let det = a * d - b * c;
        Some(((t + (t * t - 4.0 * det * det).max(0.0).sqrt()) / 2.0).sqrt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flutter: `Matrix4.identity()..setEntry(3, 2, 0.001)..rotateY(0.5)` (column-major).
    fn flutter_perspective() -> [f64; 16] {
        let (s, c) = 0.5f64.sin_cos();
        // identity with m[11] (row 3, col 2) = 0.001, then × rotateY(0.5).
        let p = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.001, 0.0, 0.0, 0.0, 1.0];
        let r = [c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, 0.0, 0.0, 0.0, 1.0];
        let mut out = [0.0; 16];
        for col in 0..4 {
            for row in 0..4 {
                out[col * 4 + row] = (0..4).map(|k| p[k * 4 + row] * r[col * 4 + k]).sum();
            }
        }
        out
    }

    #[test]
    fn from_matrix4_reduces_rows_and_columns() {
        let m = flutter_perspective();
        let h = Projective::from_matrix4(m);
        let (s, c) = 0.5f64.sin_cos();
        let want = [c, 0.0, 0.0, 0.0, 1.0, 0.0, -0.001 * s, 0.0, 1.0];
        for (g, w) in h.0.iter().zip(want) {
            assert!((g - w).abs() < 1e-12, "{:?}", h.0);
        }
    }

    #[test]
    fn inverse_roundtrips() {
        let h = Projective::from_matrix4(flutter_perspective())
            .then(&Projective::from_affine(Affine::translate((40.0, 20.0))));
        let inv = h.inverse().expect("invertible");
        let p = Point::new(12.0, 34.0);
        let q = h.map_point(p).expect("in front");
        let back = inv.map_point(q).expect("in front");
        assert!((back - p).hypot() < 1e-9, "{back:?}");
    }

    #[test]
    fn behind_camera_point_maps_to_none() {
        let h = Projective([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, -0.01, 0.0, 1.0]);
        assert!(h.map_point(Point::new(50.0, 0.0)).is_some());
        assert!(h.map_point(Point::new(150.0, 0.0)).is_none(), "w′ = 1 − 1.5 < 0");
    }
}
