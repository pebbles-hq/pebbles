//! `brush/` image brushes on fills and strokes (05 §7) — Slice P4, 128×128, judged by the
//! golden model and analytic probes (the transitional smoke reference skips image brushes).

use pebbles_render::paint::{kurbo, peniko};

use kurbo::{Affine, Rect, RoundedRect, Stroke};
use peniko::{Extend, Fill, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality};

use crate::case::{Case, Category::Image, Slice::P4, ops};
use crate::probe::{self, noise_rgba8};

pub fn cases() -> Vec<Case> {
    vec![
        ops("brush/image_fill_repeat", Image, P4, 128, 128, |p| {
            let b = noise().with_extend(Extend::Repeat);
            p.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                &b,
                Some(Affine::scale(2.)),
                &Rect::new(8., 8., 120., 120.),
            );
        })
        .probe("image_fill_texels", probe::image_fill_texels),
        ops("brush/image_fill_rrect_reflect", Image, P4, 128, 128, |p| {
            let b = noise().with_extend(Extend::Reflect);
            p.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                &b,
                Some(Affine::scale(2.)),
                &RoundedRect::new(8., 8., 120., 120., 24.),
            );
        }),
        ops("brush/image_stroke", Image, P4, 128, 128, |p| {
            let b = noise().with_extend(Extend::Repeat);
            p.stroke(
                &Stroke::new(12.),
                Affine::IDENTITY,
                &b,
                Some(Affine::scale(2.)),
                &kurbo::Circle::new((64., 64.), 44.),
            );
        }),
    ]
}

/// The 16×16 `noise_rgba8(.., 7)` image, `Low` quality (texel-exact under `scale(2)`).
fn noise() -> ImageBrush {
    ImageBrush::new(ImageData {
        data: peniko::Blob::from(noise_rgba8(16, 16, 7)),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: 16,
        height: 16,
    })
    .with_quality(ImageQuality::Low)
}
