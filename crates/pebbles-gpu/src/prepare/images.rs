//! Image preparation: one `ImageRequest` per image op — a `Blob` refcount bump, never a pixel copy.

use std::ops::Range;

use kurbo::Affine;

use super::num::{index_u32, to_f32};
use super::plan::{FramePlan, ImageRequest};
use crate::execute::resources::image_cache::{ImageKey, SamplerKey};
use crate::gpu_types::QuadInstance;

/// The 02 §8.1 sampler of an image draw (R30): quality → filter/mipmap filter, extend →
/// address modes. 10 §3.5 caches one `wgpu::Sampler` per key.
pub(crate) fn sampler_key(s: peniko::ImageSampler) -> SamplerKey {
    SamplerKey::of(s)
}

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
        sampler: sampler_key(image.sampler),
        image: image.image.clone(),
        quad,
    });
    start..start + 1
}

#[cfg(test)]
mod tests {
    use kurbo::Affine;
    use pebbles_render::paint::{Painter, Scene};
    use peniko::{Extend, ImageQuality, ImageSampler};

    use super::sampler_key;
    use crate::prepare::DrawCmd;
    use crate::prepare::test_util::prepare_scene;

    #[test]
    fn sampler_key_table() {
        let qualities = [ImageQuality::Low, ImageQuality::Medium, ImageQuality::High];
        let extends = [Extend::Pad, Extend::Repeat, Extend::Reflect];
        let addr = |e| match e {
            Extend::Pad => wgpu::AddressMode::ClampToEdge,
            Extend::Repeat => wgpu::AddressMode::Repeat,
            _ => wgpu::AddressMode::MirrorRepeat,
        };
        let mut keys = std::collections::HashSet::new();
        for q in qualities {
            for x in extends {
                for y in extends {
                    let s = ImageSampler { x_extend: x, y_extend: y, quality: q, alpha: 1.0 };
                    let k = sampler_key(s);
                    let filter = if q == ImageQuality::Low {
                        wgpu::FilterMode::Nearest
                    } else {
                        wgpu::FilterMode::Linear
                    };
                    let mip = if q == ImageQuality::High {
                        wgpu::MipmapFilterMode::Linear
                    } else {
                        wgpu::MipmapFilterMode::Nearest
                    };
                    assert_eq!(
                        (k.filter, k.mipmap, k.address_u, k.address_v),
                        (filter, mip, addr(x), addr(y)),
                        "{s:?}"
                    );
                    keys.insert(k);
                }
            }
        }
        assert_eq!(keys.len(), 27);
    }

    #[test]
    fn default_sampler_is_linear_clamp() {
        let k = sampler_key(ImageSampler::default());
        assert_eq!(k.filter, wgpu::FilterMode::Linear);
        assert_eq!(
            (k.address_u, k.address_v),
            (wgpu::AddressMode::ClampToEdge, wgpu::AddressMode::ClampToEdge)
        );
    }

    #[test]
    fn different_samplers_do_not_merge() {
        let image = crate::test_support::rgba_image(4, 4, [255, 0, 0, 255]);
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        p.draw_image(&image, Affine::translate((0.0, 0.0)));
        p.draw_image(&image.clone().with_quality(ImageQuality::Low), Affine::translate((8.0, 0.0)));
        p.draw_image(&image.clone().with_quality(ImageQuality::Low), Affine::translate((16.0, 0.0)));
        let (plan, _) = prepare_scene(&s, 64, 64);
        let images: Vec<_> = plan.cmds.iter().filter(|c| matches!(c, DrawCmd::Images { .. })).collect();
        assert_eq!(images.len(), 2, "same image, different sampler: two commands; same sampler merges");
    }
}
