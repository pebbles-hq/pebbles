//! Image preparation: one `ImageRequest` per image op — a `Blob` refcount bump, never a pixel copy.

use std::ops::Range;

use kurbo::Affine;

use super::num::{index_u32, to_f32};
use super::plan::{FramePlan, ImageRequest};
use crate::execute::resources::image_cache::{ImageKey, SamplerKey};
use crate::gpu_types::QuadInstance;

/// Why an `ImageData` cannot be drawn (R33, OC-40).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImageProblem {
    ZeroSize,
    UnknownFormat,
    LengthMismatch { expected: Option<usize>, actual: usize },
}

/// Validate before any key or upload (R33): sizes, format, and exact data length.
pub(crate) fn validate_image(d: &peniko::ImageData) -> Result<(), ImageProblem> {
    if d.width == 0 || d.height == 0 {
        return Err(ImageProblem::ZeroSize);
    }
    match d.format {
        peniko::ImageFormat::Rgba8 | peniko::ImageFormat::Bgra8 => {}
        _ => return Err(ImageProblem::UnknownFormat),
    }
    let expected = d.format.size_in_bytes(d.width, d.height);
    let actual = d.data.len();
    if expected != Some(actual) {
        return Err(ImageProblem::LengthMismatch { expected, actual });
    }
    Ok(())
}

/// A dynamic image draw (R37): like `image`, keyed by its id (never `Blob::id`), never mipmapped.
pub(crate) fn dynamic_image(
    plan: &mut FramePlan,
    frame: &pebbles_render::paint::DynamicImageFrame,
    quality: peniko::ImageQuality,
    transform: Affine,
    opacity: f32,
    snap: bool,
) -> Range<u32> {
    let data = peniko::ImageData {
        data: frame.data.clone(),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::AlphaPremultiplied,
        width: frame.width,
        height: frame.height,
    };
    // `Low` keeps nearest sampling; mips are never built for dynamic images.
    let q = if quality == peniko::ImageQuality::High { peniko::ImageQuality::Medium } else { quality };
    let brush = peniko::ImageBrush::new(data).with_quality(q);
    let range = image(plan, &brush, transform, opacity, snap);
    let key = ImageKey {
        blob_id: crate::execute::resources::dynamic_images::DYNAMIC_KEY_BIT | frame.id,
        width: frame.width,
        height: frame.height,
        format: 0,
        alpha: 1,
    };
    for req in plan.images.get_mut(range.start as usize..range.end as usize).unwrap_or_default() {
        req.key = key;
        req.mips = false;
        req.quad.color[0] = 0.0; // LOD 0: one level
        req.dynamic = Some(frame.clone());
    }
    range
}

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
    // R35: the LOD is computed once per draw on the CPU from the image px → device map
    // (`Medium` = the nearest level, `High` = trilinear; `Low` keeps level 0). Image quads carry it
    // in `color.x` (their `color.rgb` is otherwise unused; `.a` is the opacity).
    let quality = image.sampler.quality;
    let mips = quality != peniko::ImageQuality::Low;
    let lod = if mips {
        let l = crate::execute::resources::image_mips::lod(x.x / w, x.y / w, y.x / h, y.y / h);
        if quality == peniko::ImageQuality::Medium { l.round() } else { l }
    } else {
        0.0
    };
    let quad = QuadInstance {
        xform: [f(x.x), f(x.y), f(y.x), f(y.y)],
        origin: [f(o.x), f(o.y)],
        uv: [0.0, 0.0, 1.0, 1.0],
        color: [f(lod), 0.0, 0.0, opacity],
        layer_kind: [super::image_nine::EDGE_ALL, 2],
    };
    let start = index_u32(plan.images.len());
    plan.images.push(ImageRequest {
        key: ImageKey::of(&image.image),
        sampler: sampler_key(image.sampler),
        image: image.image.clone(),
        quad,
        mips,
        dynamic: None,
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

    use super::{ImageProblem, validate_image};

    fn data(w: u32, h: u32, len: usize) -> peniko::ImageData {
        peniko::ImageData {
            data: peniko::Blob::from(vec![0u8; len]),
            format: peniko::ImageFormat::Rgba8,
            alpha_type: peniko::ImageAlphaType::Alpha,
            width: w,
            height: h,
        }
    }

    #[test]
    fn zero_size_skipped() {
        assert_eq!(validate_image(&data(0, 4, 0)), Err(ImageProblem::ZeroSize));
        assert_eq!(validate_image(&data(4, 0, 0)), Err(ImageProblem::ZeroSize));
    }

    #[test]
    fn short_data_skipped() {
        let e = validate_image(&data(4, 4, 60));
        assert_eq!(e, Err(ImageProblem::LengthMismatch { expected: Some(64), actual: 60 }));
        // And prepare draws nothing, recording the fallback.
        let mut s = Scene::new();
        Painter::new(&mut s).draw_image(&peniko::ImageBrush::new(data(4, 4, 60)), Affine::IDENTITY);
        let (plan, stats) = prepare_scene(&s, 16, 16);
        assert!(plan.cmds.is_empty() && plan.images.is_empty());
        assert_eq!(stats.unsupported[crate::stats::Unsupported::ImageMalformed as usize], 1);
    }

    #[test]
    fn long_data_skipped() {
        assert!(matches!(validate_image(&data(4, 4, 65)), Err(ImageProblem::LengthMismatch { .. })));
    }

    #[test]
    fn overflowing_size_skipped() {
        let e = validate_image(&data(u32::MAX, u32::MAX, 16));
        assert_eq!(e, Err(ImageProblem::LengthMismatch { expected: None, actual: 16 }));
    }

    #[test]
    fn valid_image_passes() {
        assert_eq!(validate_image(&data(4, 4, 64)), Ok(()));
    }

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
