//! Nine-slice images (02 R36, Flutter `drawImageNine`): at most nine image quads, each with its
//! own source rect (UVs inset by half a texel so no slice samples across its border), merged
//! into one instanced command by the normal ORD-3 rule. Internal edges are not antialiased
//! (top-left rule), so adjacent slices tile without seams; the outer edges are.

use kurbo::{Affine, Rect};

use super::num::{index_u32, to_f32};
use super::plan::{FramePlan, ImageRequest};
use crate::execute::resources::image_cache::ImageKey;
use crate::gpu_types::QuadInstance;

/// Edge-mask bits of an image quad (`QuadInstance.layer_kind.x` for images): an edge whose
/// bit is set is antialiased; a clear bit uses the top-left inclusion rule.
pub(crate) const EDGE_U0: u32 = 1;
pub(crate) const EDGE_U1: u32 = 2;
pub(crate) const EDGE_V0: u32 = 4;
pub(crate) const EDGE_V1: u32 = 8;
pub(crate) const EDGE_ALL: u32 = 15;

/// The R36 step 2 split: `(source, destination, edge mask)` per non-empty slice, and whether the
/// center had to be clamped into the image.
pub(crate) fn nine_slices(w: f64, h: f64, center: Rect, dst: Rect) -> (Vec<(Rect, Rect, u32)>, bool) {
    let c = Rect::new(
        center.x0.clamp(0.0, w),
        center.y0.clamp(0.0, h),
        center.x1.clamp(0.0, w),
        center.y1.clamp(0.0, h),
    );
    let clamped = c != center;
    // Columns: source [0, cx0, cx1, w] → destination [d0, d0 + cx0, d1 − (w − cx1), d1]; when
    // the fixed corners do not fit, both scale by len / corners and the center is dropped.
    let axis = |n: f64, c0: f64, c1: f64, d0: f64, d1: f64| -> ([f64; 4], [f64; 4]) {
        let corners = c0 + (n - c1);
        let len = d1 - d0;
        let dst = if corners > len && corners > 0.0 {
            let k = len / corners;
            [d0, d0 + c0 * k, d0 + c0 * k, d1]
        } else {
            [d0, d0 + c0, d1 - (n - c1), d1]
        };
        ([0.0, c0, c1, n], dst)
    };
    let (sx, dx) = axis(w, c.x0, c.x1, dst.x0, dst.x1);
    let (sy, dy) = axis(h, c.y0, c.y1, dst.y0, dst.y1);
    let mut out = Vec::with_capacity(9);
    for (ys, yd) in sy.windows(2).zip(dy.windows(2)) {
        let ([sy0, sy1], [dy0, dy1]) = (ys, yd) else { continue };
        for (xs, xd) in sx.windows(2).zip(dx.windows(2)) {
            let ([sx0, sx1], [dx0, dx1]) = (xs, xd) else { continue };
            let s = Rect::new(*sx0, *sy0, *sx1, *sy1);
            let d = Rect::new(*dx0, *dy0, *dx1, *dy1);
            if s.width() <= 0.0 || s.height() <= 0.0 || d.width() <= 0.0 || d.height() <= 0.0 {
                continue;
            }
            // Antialias only the edges on the destination's outline.
            let mut edges = 0;
            if d.x0 <= dst.x0 {
                edges |= EDGE_U0;
            }
            if d.x1 >= dst.x1 {
                edges |= EDGE_U1;
            }
            if d.y0 <= dst.y0 {
                edges |= EDGE_V0;
            }
            if d.y1 >= dst.y1 {
                edges |= EDGE_V1;
            }
            out.push((s, d, edges));
        }
    }
    (out, clamped)
}

/// Push the slices of one nine-slice draw under `transform`; returns the request range and
/// whether the center was clamped.
pub(crate) fn image_nine(
    plan: &mut FramePlan,
    image: &peniko::ImageBrush,
    center: Rect,
    dst: Rect,
    transform: Affine,
    opacity: f32,
) -> (std::ops::Range<u32>, bool) {
    let (w, h) = (f64::from(image.image.width), f64::from(image.image.height));
    let (slices, clamped) = nine_slices(w, h, center, dst.abs());
    let start = index_u32(plan.images.len());
    let f = |v: f64| to_f32(v).unwrap_or(0.0);
    let key = ImageKey::of(&image.image);
    let sampler = super::images::sampler_key(image.sampler);
    for (s, d, edges) in slices {
        let o = transform * kurbo::Point::new(d.x0, d.y0);
        let x = transform * kurbo::Point::new(d.x1, d.y0) - o;
        let y = transform * kurbo::Point::new(d.x0, d.y1) - o;
        // UVs inset by half a texel inside the slice (no bleeding across slices).
        let inset = |a: f64, b: f64, n: f64| -> (f64, f64) {
            let half = 0.5_f64.min(0.5 * (b - a));
            ((a + half) / n, (b - half) / n)
        };
        let (u0, u1) = inset(s.x0, s.x1, w);
        let (v0, v1) = inset(s.y0, s.y1, h);
        let quad = QuadInstance {
            xform: [f(x.x), f(x.y), f(y.x), f(y.y)],
            origin: [f(o.x), f(o.y)],
            uv: [f(u0), f(v0), f(u1), f(v1)],
            color: [0.0, 0.0, 0.0, opacity],
            layer_kind: [edges, 2],
        };
        plan.images.push(ImageRequest { key, sampler, image: image.image.clone(), quad, mips: false });
    }
    (start..index_u32(plan.images.len()), clamped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nine_slice_splits_into_nine() {
        let (s, clamped) =
            nine_slices(64.0, 64.0, Rect::new(16.0, 16.0, 48.0, 48.0), Rect::new(0.0, 0.0, 200.0, 100.0));
        assert!(!clamped);
        let dst: Vec<Rect> = s.iter().map(|x| x.1).collect();
        assert_eq!(
            dst,
            vec![
                Rect::new(0.0, 0.0, 16.0, 16.0),
                Rect::new(16.0, 0.0, 184.0, 16.0),
                Rect::new(184.0, 0.0, 200.0, 16.0),
                Rect::new(0.0, 16.0, 16.0, 84.0),
                Rect::new(16.0, 16.0, 184.0, 84.0),
                Rect::new(184.0, 16.0, 200.0, 84.0),
                Rect::new(0.0, 84.0, 16.0, 100.0),
                Rect::new(16.0, 84.0, 184.0, 100.0),
                Rect::new(184.0, 84.0, 200.0, 100.0),
            ]
        );
        assert_eq!(s[0].2, EDGE_U0 | EDGE_V0, "outer edges only");
        assert_eq!(s[4].2, 0, "the center has no outer edge");
    }

    #[test]
    fn nine_slice_small_dst_scales_corners() {
        let (s, _) =
            nine_slices(64.0, 64.0, Rect::new(16.0, 16.0, 48.0, 48.0), Rect::new(0.0, 0.0, 20.0, 20.0));
        assert_eq!(s.len(), 4, "center row and column dropped");
        assert_eq!(s[0].1, Rect::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!(s[3].1, Rect::new(10.0, 10.0, 20.0, 20.0));
    }

    #[test]
    fn nine_slice_clamps_center() {
        let (s, clamped) =
            nine_slices(64.0, 64.0, Rect::new(-10.0, 16.0, 48.0, 90.0), Rect::new(0.0, 0.0, 200.0, 100.0));
        assert!(clamped);
        assert!(s.iter().all(|x| x.0.x0 >= 0.0 && x.0.y1 <= 64.0));
    }
}
