//! Image preparation: one `ImageRequest` per image op — a `Blob` refcount bump, never a pixel copy.

use std::ops::Range;

use kurbo::Affine;

use super::num::{index_u32, to_f32};
use super::plan::{FramePlan, ImageRequest};
use crate::execute::resources::image_cache::{ImageKey, SamplerKey};
use crate::gpu_types::QuadInstance;

/// Append one image request (placed in device space) and return its range. With `snap`
/// (02 §5.3, R18/R19), an image whose device transform is a pure translation with 1 texel =
/// 1 device px is drawn at an integer origin: every pixel center then lands on a texel center,
/// so the output is texel-exact.
pub(crate) fn image(
    plan: &mut FramePlan,
    image: &peniko::ImageBrush,
    transform: Affine,
    opacity: f32,
    snap: bool,
) -> Range<u32> {
    let (w, h) = (f64::from(image.image.width), f64::from(image.image.height));
    let [a, b, c, d, e, f] = transform.as_coeffs();
    let unit_translation = a == 1.0 && b == 0.0 && c == 0.0 && d == 1.0;
    let transform = if snap && unit_translation {
        Affine::translate(((e + 0.5).floor(), (f + 0.5).floor()))
    } else {
        transform
    };
    let o = transform * kurbo::Point::ORIGIN;
    let x = transform * kurbo::Point::new(w, 0.0) - o;
    let y = transform * kurbo::Point::new(0.0, h) - o;
    let f = |v: f64| to_f32(v).unwrap_or(0.0);
    let quad = QuadInstance {
        xform: [f(x.x), f(x.y), f(y.x), f(y.y)],
        origin: [f(o.x), f(o.y)],
        uv: [0.0, 0.0, 1.0, 1.0],
        color: [0.0, 0.0, 0.0, opacity],
        layer_kind: [0, 2],
    };
    let start = index_u32(plan.images.len());
    plan.images.push(ImageRequest {
        key: ImageKey::of(&image.image),
        sampler: SamplerKey,
        image: image.image.clone(),
        quad,
    });
    start..start + 1
}
