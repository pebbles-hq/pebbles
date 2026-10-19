//! img/ — Slice P3 (V23), §7.2. Procedural images only (never files or network): checker =
//! 8×8 cells `#1F2937`/`#F9FAFB`, noise = `XorShift64` (seed 7).

use pebbles_core::IntoWidget as _;
use pebbles_render::paint::{Painter, kurbo, peniko};
use pebbles_render::{BorderRadius, BoxDecoration, ImageFit, image_from_rgba8};
use pebbles_testing::Harness;
use pebbles_widgets::container;

use kurbo::{Affine, Rect, RoundedRect};
use peniko::{Extend, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality};

use crate::case::{
    Case, Category::Analytic, Category::Image, Slice::P3, Slice::P4, harness, ops, with_scale_variants,
};
use crate::probe::{self, noise_rgba8};

const W: u32 = 128;
const H: u32 = 128;

pub fn cases() -> Vec<Case> {
    let mut v = vec![ops("img/aligned_1x", crate::case::Category::Exact, P3, W, H, aligned_1x)];
    v.extend(with_scale_variants(ops("img/scaled_up_3x", Image, P3, W, H, scaled_up_3x)));
    v.extend([
        ops("img/scaled_down_quarter", Image, P3, W, H, scaled_down_quarter),
        ops("img/nonuniform_scale", Image, P3, W, H, nonuniform_scale),
        ops("img/rotated_30", Image, P3, W, H, rotated_30),
        ops("img/quality_low_nearest", Image, P3, W, H, quality_low_nearest),
        ops("img/straight_alpha", Image, P3, W, H, straight_alpha),
        ops("img/premultiplied_alpha", Image, P3, W, H, premultiplied_alpha),
        ops("img/bgra8_source", Image, P3, W, H, bgra8_source),
        ops("img/brush_alpha_0_5", Image, P3, W, H, brush_alpha_0_5),
        ops("img/edge_sampling", Image, P3, W, H, edge_sampling),
        harness("img/fit_cover_rrect", Image, P3, 200, 120, fit_cover_rrect),
        ops("img/huge_10000x300", Analytic, P3, 1000, 30, huge_10000x300)
            .analytic()
            .probe("huge_image_downscaled_red", probe::huge_image_downscaled_red),
        ops("img/many_200_distinct", Image, P3, 256, 256, many_200_distinct),
        // R37 dynamic image (V53 case): updated in place, drawn like an image.
        ops("img/dynamic_update", Image, P3, W, H, dynamic_update),
        // L40 image brushes (09 §8.0): judged by the golden model and analytic probes.
        ops("img/brush_fill_pad", Image, P4, 64, 64, |p| brush_fill(p, Extend::Pad, 1.0))
            .probe("brush_fill_pad", probe::brush_fill_pad),
        ops("img/brush_fill_repeat", Image, P4, 64, 64, |p| brush_fill(p, Extend::Repeat, 1.0))
            .probe("brush_fill_repeat", probe::brush_fill_repeat),
        ops("img/brush_fill_reflect", Image, P4, 64, 64, |p| brush_fill(p, Extend::Reflect, 1.0))
            .probe("brush_fill_reflect", probe::brush_fill_reflect),
        ops("img/brush_fill_alpha_0_5", Image, P4, 64, 64, |p| brush_fill(p, Extend::Pad, 0.5))
            .probe("brush_alpha_half", probe::brush_alpha_half),
        ops("img/brush_stroke", Image, P4, 64, 64, |p| {
            let rr = RoundedRect::new(12.0, 12.0, 52.0, 52.0, 8.0);
            let b = brush_2x2(Extend::Repeat, 1.0);
            p.stroke(&kurbo::Stroke::new(16.0), Affine::IDENTITY, &b, Some(Affine::scale(16.0)), &rr);
        }),
        ops("img/brush_transform_rotate", Image, P4, 64, 64, |p| {
            let b = brush_2x2(Extend::Repeat, 1.0);
            let xf = Affine::rotate(std::f64::consts::FRAC_PI_4) * Affine::scale(0.5) * Affine::scale(16.0);
            p.fill(peniko::Fill::NonZero, Affine::IDENTITY, &b, Some(xf), &Rect::new(0.0, 0.0, 64.0, 64.0));
        }),
        // R36 nine-slice (V53 case): a bordered 24×24 "button" stretched to 112×40.
        ops("img/nine_slice_button", Image, P3, W, H, nine_slice_button),
        // R35 mip chains (V53 cases): a one-pixel checker minified ×0.3.
        ops("img/minify_checker_medium", Image, P3, W, H, minify_checker_medium),
        ops("img/minify_checker_high", Image, P3, W, H, minify_checker_high),
        ops("img/under_clip_and_opacity", Image, P3, W, H, under_clip_and_opacity),
    ]);
    v
}

/// `w`×`h` straight RGBA8 checker with `cell`-px cells.
pub fn checker_rgba8(w: u32, h: u32, cell: u32) -> Vec<u8> {
    let (dark, light) = ([0x1F, 0x29, 0x37, 0xFF], [0xF9, 0xFA, 0xFB, 0xFF]);
    (0..h)
        .flat_map(|y| (0..w).map(move |x| if (x / cell + y / cell).is_multiple_of(2) { dark } else { light }))
        .flatten()
        .collect()
}

fn checker(w: u32, h: u32, cell: u32) -> ImageBrush {
    image_from_rgba8(w, h, checker_rgba8(w, h, cell))
}

fn noise(w: u32, h: u32, seed: u64) -> ImageBrush {
    image_from_rgba8(w, h, noise_rgba8(w, h, seed))
}

/// `px` with every pixel's alpha set to 128 (straight).
fn half_alpha(mut px: Vec<u8>) -> Vec<u8> {
    for p in px.as_chunks_mut::<4>().0 {
        p[3] = 128;
    }
    px
}

/// Image data with an explicit format / alpha type.
fn data(w: u32, h: u32, px: Vec<u8>, format: ImageFormat, alpha_type: ImageAlphaType) -> ImageBrush {
    ImageBrush::new(ImageData { data: peniko::Blob::from(px), format, alpha_type, width: w, height: h })
}

fn aligned_1x(p: &mut Painter<'_>) {
    p.draw_image(&checker(64, 64, 8), Affine::translate((32.0, 32.0)));
}

fn scaled_up_3x(p: &mut Painter<'_>) {
    let img = noise(16, 16, 7).with_quality(ImageQuality::Medium);
    p.draw_image(&img, Affine::translate((40.0, 40.0)) * Affine::scale(3.0));
}

fn scaled_down_quarter(p: &mut Painter<'_>) {
    p.draw_image(&checker(256, 256, 8), Affine::translate((32.0, 32.0)) * Affine::scale(0.25));
}

fn nonuniform_scale(p: &mut Painter<'_>) {
    p.draw_image(&noise(32, 32, 7), Affine::translate((16.0, 40.0)) * Affine::scale_non_uniform(3.0, 1.5));
}

fn rotated_30(p: &mut Painter<'_>) {
    let center = kurbo::Point::new(64.0, 64.0);
    p.draw_image(
        &checker(64, 64, 8),
        Affine::rotate_about(std::f64::consts::FRAC_PI_6, center) * Affine::translate((32.0, 32.0)),
    );
}

fn quality_low_nearest(p: &mut Painter<'_>) {
    let img = noise(8, 8, 7).with_quality(ImageQuality::Low);
    p.draw_image(&img, Affine::translate((16.0, 16.0)) * Affine::scale(12.0));
}

fn straight_alpha(p: &mut Painter<'_>) {
    let px = half_alpha(noise_rgba8(32, 32, 7));
    p.draw_image(
        &data(32, 32, px, ImageFormat::Rgba8, ImageAlphaType::Alpha),
        Affine::translate((16.0, 16.0)) * Affine::scale(3.0),
    );
}

fn premultiplied_alpha(p: &mut Painter<'_>) {
    let mut px = half_alpha(noise_rgba8(32, 32, 7));
    for q in px.as_chunks_mut::<4>().0 {
        for c in &mut q[..3] {
            *c = u8::try_from((u16::from(*c) * 128 + 127) / 255).unwrap_or(u8::MAX);
        }
    }
    let img = data(32, 32, px, ImageFormat::Rgba8, ImageAlphaType::AlphaPremultiplied);
    p.draw_image(&img, Affine::translate((16.0, 16.0)) * Affine::scale(3.0));
}

fn bgra8_source(p: &mut Painter<'_>) {
    let mut px = noise_rgba8(32, 32, 7);
    for q in px.as_chunks_mut::<4>().0 {
        q.swap(0, 2);
    }
    p.draw_image(
        &data(32, 32, px, ImageFormat::Bgra8, ImageAlphaType::Alpha),
        Affine::translate((16.0, 16.0)) * Affine::scale(3.0),
    );
}

fn brush_alpha_0_5(p: &mut Painter<'_>) {
    p.draw_image(&checker(64, 64, 8).with_alpha(0.5), Affine::translate((32.0, 32.0)));
}

fn edge_sampling(p: &mut Painter<'_>) {
    let pad = noise(4, 4, 7).with_quality(ImageQuality::Medium).with_extend(Extend::Pad);
    let repeat = noise(4, 4, 7).with_quality(ImageQuality::Medium).with_extend(Extend::Repeat);
    p.draw_image(&pad, Affine::translate((8.0, 24.0)) * Affine::scale(20.0));
    p.draw_image(&repeat, Affine::translate((40.0, 24.0)) * Affine::scale(20.0));
}

fn fit_cover_rrect(h: &mut Harness) {
    let deco = BoxDecoration::new()
        .image(checker(64, 64, 8))
        .image_fit(ImageFit::Cover)
        .radius(BorderRadius::all(16.0));
    h.mount_widget(container().width(200.0).height(120.0).decoration(deco).into_widget());
}

fn huge_10000x300(p: &mut Painter<'_>) {
    let red = image_from_rgba8(10_000, 300, [0xFF, 0, 0, 0xFF].repeat(10_000 * 300));
    p.draw_image(&red, Affine::scale(0.1));
}

fn many_200_distinct(p: &mut Painter<'_>) {
    for i in 0..200u32 {
        let (x, y) = (f64::from(i % 15) * 17.0, f64::from(i / 15) * 17.0);
        p.draw_image(&noise(16, 16, 1000 + u64::from(i)), Affine::translate((x, y)));
    }
}

fn dynamic_update(p: &mut Painter<'_>) {
    let mut img = pebbles_render::paint::DynamicImage::new(32, 32);
    // Premultiplied noise as the "new frame", one dirty band.
    img.update(
        peniko::Blob::from(noise_rgba8(32, 32, 9)),
        Some(pebbles_render::paint::IRect { x: 0, y: 8, w: 32, h: 16 }),
    );
    p.draw_dynamic_image(
        &img,
        ImageQuality::Medium,
        1.0,
        Affine::translate((16.0, 16.0)) * Affine::scale(3.0),
    );
}

/// A 24×24 button skin: 6-px dark border, light interior, a distinct color per corner.
fn button_skin() -> ImageBrush {
    let px: Vec<u8> = (0..24u32 * 24)
        .flat_map(|i| {
            let (x, y) = (i % 24, i / 24);
            let border = !(6..18).contains(&x) || !(6..18).contains(&y);
            match (x < 6, x >= 18, y < 6, y >= 18) {
                (true, _, true, _) => [0xEF, 0x44, 0x44, 0xFF],
                (_, true, true, _) => [0x10, 0xB9, 0x81, 0xFF],
                (true, _, _, true) => [0x3B, 0x82, 0xF6, 0xFF],
                (_, true, _, true) => [0xF5, 0x9E, 0x0B, 0xFF],
                _ if border => [0x1F, 0x29, 0x37, 0xFF],
                _ => [0xF9, 0xFA, 0xFB, 0xFF],
            }
        })
        .collect();
    image_from_rgba8(24, 24, px)
}

fn nine_slice_button(p: &mut Painter<'_>) {
    let dst = Rect::new(8.0, 40.0, 120.0, 80.0);
    p.draw_image_nine(&button_skin(), Rect::new(6.0, 6.0, 18.0, 18.0), dst, Affine::IDENTITY);
}

fn minify_checker(p: &mut Painter<'_>, q: ImageQuality) {
    let img = checker(320, 320, 1).with_quality(q);
    p.draw_image(&img, Affine::translate((16.0, 16.0)) * Affine::scale(0.3));
}

fn minify_checker_medium(p: &mut Painter<'_>) {
    minify_checker(p, ImageQuality::Medium);
}

fn minify_checker_high(p: &mut Painter<'_>) {
    minify_checker(p, ImageQuality::High);
}

fn under_clip_and_opacity(p: &mut Painter<'_>) {
    let clip = RoundedRect::from_rect(Rect::new(16.0, 16.0, 112.0, 112.0), 24.0);
    p.push_layer(
        peniko::Fill::NonZero,
        peniko::Mix::Normal,
        0.5,
        Affine::IDENTITY,
        &Rect::new(0.0, 0.0, 128.0, 128.0),
    );
    p.push_layer(peniko::Fill::NonZero, peniko::Mix::Normal, 1.0, Affine::IDENTITY, &clip);
    p.draw_image(&checker(128, 128, 8), Affine::IDENTITY);
    p.pop_layer();
    p.pop_layer();
}

/// 09 §8.0: the 2×2 brush image (red, green / blue, white), `Low` quality.
fn brush_2x2(extend: Extend, alpha: f32) -> ImageBrush {
    let px: Vec<u8> = [[255u8, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 255, 255]].concat();
    let data = ImageData {
        data: peniko::Blob::from(px),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: 2,
        height: 2,
    };
    ImageBrush::new(data).with_quality(ImageQuality::Low).with_extend(extend).with_alpha(alpha)
}

/// A canvas-filling rect painted with the 2×2 brush under `scale(16)`.
fn brush_fill(p: &mut Painter<'_>, extend: Extend, alpha: f32) {
    let b = brush_2x2(extend, alpha);
    p.fill(
        peniko::Fill::NonZero,
        Affine::IDENTITY,
        &b,
        Some(Affine::scale(16.0)),
        &Rect::new(0.0, 0.0, 64.0, 64.0),
    );
}
