//! img/ — Slice P3 (V23), §7.2. Procedural images only (never files or network): checker =
//! 8×8 cells `#1F2937`/`#F9FAFB`, noise = `XorShift64` (seed 7).

use pebbles_core::IntoWidget as _;
use pebbles_render::paint::{Painter, kurbo, peniko};
use pebbles_render::{BorderRadius, BoxDecoration, ImageFit, image_from_rgba8};
use pebbles_testing::Harness;
use pebbles_widgets::container;

use kurbo::{Affine, Rect, RoundedRect};
use peniko::{Extend, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality};

use crate::case::{Case, Category::Analytic, Category::Image, Slice::P3, harness, ops, with_scale_variants};
use crate::probe::{self, noise_rgba8};

const W: u32 = 128;
const H: u32 = 128;

pub fn cases() -> Vec<Case> {
    let mut v = vec![ops("img/aligned_1x", crate::case::Category::Exact, P3, W, H, aligned_1x)];
    v.extend(with_scale_variants(ops("img/scaled_up_3x", Image, P3, W, H, scaled_up_3x)));
    v.extend([
        ops("img/scaled_down_quarter", Image, P3, W, H, scaled_down_quarter),
        ops("img/nonuniform_scale", Image, P3, W, H, nonuniform_scale),
        // Interior texels match the golden model (Δ ≤ 3); the rotated quad's edges are not
        // antialiased yet (Δ 113 on the edge pixels).
        ops("img/rotated_30", Image, P3, W, H, rotated_30)
            .xfail("R31", "rotated image quad edges are not antialiased until R31"),
        ops("img/quality_low_nearest", Image, P3, W, H, quality_low_nearest),
        ops("img/straight_alpha", Image, P3, W, H, straight_alpha),
        ops("img/premultiplied_alpha", Image, P3, W, H, premultiplied_alpha),
        ops("img/bgra8_source", Image, P3, W, H, bgra8_source),
        ops("img/brush_alpha_0_5", Image, P3, W, H, brush_alpha_0_5),
        ops("img/edge_sampling", Image, P3, W, H, edge_sampling),
        harness("img/fit_cover_rrect", Image, P3, 200, 120, fit_cover_rrect)
            .xfail("L01", "rounded-rect clip of a decoration image lands with L01"),
        ops("img/huge_10000x300", Analytic, P3, 1000, 30, huge_10000x300)
            .analytic()
            .probe("huge_image_downscaled_red", probe::huge_image_downscaled_red),
        ops("img/many_200_distinct", Image, P3, 256, 256, many_200_distinct),
        ops("img/under_clip_and_opacity", Image, P3, W, H, under_clip_and_opacity)
            .xfail("L10", "exact group opacity lands with L10"),
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
