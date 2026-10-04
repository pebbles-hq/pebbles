//! Shadow preparation: build the inflated analytic shadow quads.

use kurbo::{Affine, Rect};
use peniko::Color;

use super::Tess;
use super::color::premul;
use crate::gpu_types::ShadowVertex;

impl Tess {
    pub(crate) fn shadow(&mut self, rect: &Rect, transform: Affine, radius: f64, std_dev: f64, color: Color) {
        let coeffs = transform.as_coeffs();
        let scale = (coeffs[0] * coeffs[0] + coeffs[1] * coeffs[1]).sqrt().max(1e-6);
        let c00 = transform * kurbo::Point::new(rect.x0, rect.y0);
        let c11 = transform * kurbo::Point::new(rect.x1, rect.y1);
        let c10 = transform * kurbo::Point::new(rect.x1, rect.y0);
        let c01 = transform * kurbo::Point::new(rect.x0, rect.y1);
        let rect = [
            c00.x.min(c10.x).min(c11.x).min(c01.x) as f32,
            c00.y.min(c10.y).min(c11.y).min(c01.y) as f32,
            c00.x.max(c10.x).max(c11.x).max(c01.x) as f32,
            c00.y.max(c10.y).max(c11.y).max(c01.y) as f32,
        ];
        let radius = (radius * scale) as f32;
        let sigma = (std_dev * scale) as f32;
        let color = premul(color);
        let extend = 3.0 * sigma;
        let quad = [
            [rect[0] - extend, rect[1] - extend],
            [rect[2] + extend, rect[1] - extend],
            [rect[2] + extend, rect[3] + extend],
            [rect[0] - extend, rect[3] + extend],
        ];
        let vert = |p: [f32; 2]| ShadowVertex { position: p, rect, radius, sigma, color };
        self.shadows.extend_from_slice(&[
            vert(quad[0]),
            vert(quad[1]),
            vert(quad[2]),
            vert(quad[0]),
            vert(quad[2]),
            vert(quad[3]),
        ]);
    }
}
