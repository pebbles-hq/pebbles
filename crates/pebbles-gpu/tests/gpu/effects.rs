//! Effect layers (09 §10, L61/L63/L64): blur, color matrix, morphology and chains on the GPU.

use super::*;
use pebbles_render::paint::display_list::{BlurEdge, LayerEffect};

fn render(name: &str, size: (u32, u32), build: impl FnOnce(&mut Painter<'_>)) -> Option<Vec<u8>> {
    let (device, queue, mut r) = setup(name)?;
    let mut s = Scene::new();
    build(&mut Painter::new(&mut s));
    Some(render_scene(&device, &queue, &mut r, &s, size, Color::WHITE))
}

fn effect(p: &mut Painter<'_>, e: LayerEffect, body: impl FnOnce(&mut Painter<'_>)) {
    p.push_effect_layer(e, None::<&Rect>, Affine::IDENTITY);
    body(p);
    p.pop_layer();
}

fn blur(sigma: f32, edge: BlurEdge) -> LayerEffect {
    LayerEffect::Blur { sigma_x: sigma, sigma_y: sigma, edge }
}

fn black_square(p: &mut Painter<'_>) {
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &Rect::new(24.0, 24.0, 40.0, 40.0));
}

/// The 09 §10.2 kernel on one channel (independent of the shader): box downsample, normalized
/// Gaussian (Decal), bilinear upsample, every pass rounded to 8 bits.
fn reference_blur(src: &[f32], w: usize, h: usize, sigma: f32) -> Vec<f32> {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() / 255.0;
    let mut k = 0;
    while k < 3 && sigma / (1u32 << k) as f32 > 2.0 {
        k += 1;
    }
    let sk = sigma / (1u32 << k) as f32;
    let mut levels = vec![(src.to_vec(), w, h)];
    for _ in 0..k {
        let (b, lw, lh) = levels.last().cloned().unwrap();
        let (ow, oh) = (lw.div_ceil(2), lh.div_ceil(2));
        let at = |x: usize, y: usize| b[y.min(lh - 1) * lw + x.min(lw - 1)];
        let d = (0..oh)
            .flat_map(|y| (0..ow).map(move |x| (x, y)))
            .map(|(x, y)| {
                q((at(2 * x, 2 * y) + at(2 * x + 1, 2 * y) + at(2 * x, 2 * y + 1) + at(2 * x + 1, 2 * y + 1))
                    / 4.0)
            })
            .collect();
        levels.push((d, ow, oh));
    }
    let (mut buf, lw, lh) = levels.pop().unwrap();
    let r = (3.0 * sk).ceil() as i64;
    let wts: Vec<f32> = (-r..=r).map(|i| (-((i * i) as f32) / (2.0 * sk * sk)).exp()).collect();
    let sum: f32 = wts.iter().sum();
    for axis in 0..2 {
        let src = buf.clone();
        for y in 0..lh as i64 {
            for x in 0..lw as i64 {
                let mut acc = 0.0;
                for (j, wt) in wts.iter().enumerate() {
                    let o = j as i64 - r;
                    let (sx, sy) = if axis == 0 { (x + o, y) } else { (x, y + o) };
                    if sx >= 0 && sy >= 0 && (sx as usize) < lw && (sy as usize) < lh {
                        acc += src[sy as usize * lw + sx as usize] * wt / sum;
                    }
                }
                buf[y as usize * lw + x as usize] = q(acc);
            }
        }
    }
    let (mut cur, mut cw, mut ch) = (buf, lw, lh);
    while let Some((_, tw, th)) = levels.pop() {
        let at = |x: i64, y: i64| {
            cur[(y.clamp(0, ch as i64 - 1) as usize) * cw + x.clamp(0, cw as i64 - 1) as usize]
        };
        let mut out = Vec::with_capacity(tw * th);
        for y in 0..th {
            for x in 0..tw {
                let (u, v) = ((x as f32 + 0.5) / 2.0 - 0.5, (y as f32 + 0.5) / 2.0 - 0.5);
                let (x0, y0) = (u.floor(), v.floor());
                let (fx, fy) = (u - x0, v - y0);
                let (i, j) = (x0 as i64, y0 as i64);
                let top = at(i, j) * (1.0 - fx) + at(i + 1, j) * fx;
                let bot = at(i, j + 1) * (1.0 - fx) + at(i + 1, j + 1) * fx;
                out.push(q(top * (1.0 - fy) + bot * fy));
            }
        }
        (cur, cw, ch) = (out, tw, th);
    }
    cur
}

#[test]
#[ignore = "gpu"]
fn blur_of_square_matches_reference_kernel() {
    let Some(px) = render("blur_of_square_matches_reference_kernel", (64, 64), |p| {
        effect(p, blur(5.0, BlurEdge::Decal), black_square)
    }) else {
        return;
    };
    let alpha: Vec<f32> = (0..64 * 64)
        .map(|i| if (24..40).contains(&(i % 64)) && (24..40).contains(&(i / 64)) { 1.0 } else { 0.0 })
        .collect();
    let want = reference_blur(&alpha, 64, 64, 5.0);
    let mut worst = 0i32;
    for y in 0..64u32 {
        for x in 0..64u32 {
            let a = want[(y * 64 + x) as usize];
            let expect = (255.0 * (1.0 - a)).round() as i32;
            worst = worst.max((i32::from(pixel(&px, 64, x, y)[0]) - expect).abs());
        }
    }
    assert!(worst <= 2, "max deviation {worst}");
    assert!(pixel(&px, 64, 32, 32)[0] < 128, "center stays dark");
}

#[test]
#[ignore = "gpu"]
fn zero_sigma_is_identity() {
    let plain = render("zero_sigma_is_identity/plain", (64, 64), black_square);
    let blurred = render("zero_sigma_is_identity/blur0", (64, 64), |p| {
        effect(p, blur(0.0, BlurEdge::Decal), black_square)
    });
    let (Some(a), Some(b)) = (plain, blurred) else { return };
    assert!(a == b);
}

#[test]
#[ignore = "gpu"]
fn decal_vs_clamp_edges() {
    let red = |p: &mut Painter<'_>| {
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(255, 0, 0, 255),
            None,
            &Rect::new(0.0, 0.0, 64.0, 64.0),
        );
    };
    let Some(decal) =
        render("decal_vs_clamp_edges/decal", (64, 64), |p| effect(p, blur(4.0, BlurEdge::Decal), red))
    else {
        return;
    };
    let Some(clamp) =
        render("decal_vs_clamp_edges/clamp", (64, 64), |p| effect(p, blur(4.0, BlurEdge::Clamp), red))
    else {
        return;
    };
    assert!(pixel(&decal, 64, 0, 32)[1] > 60, "decal: the edge blends with transparent");
    assert_eq!(pixel(&clamp, 64, 0, 32), [255, 0, 0, 255], "clamp: the edge stays red");
    assert_eq!(pixel(&decal, 64, 32, 32), [255, 0, 0, 255]);
}

#[test]
#[ignore = "gpu"]
fn blurred_fill_larger_than_mask_max_area_renders() {
    let Some((device, queue, mut r)) = setup("blurred_fill_larger_than_mask_max_area_renders") else {
        return;
    };
    let mut s = Scene::new();
    Painter::new(&mut s).fill_blurred(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::BLACK,
        &Rect::new(100.0, 100.0, 700.0, 700.0),
        8.0,
        pebbles_render::paint::display_list::BlurStyle::Normal,
    );
    let px = render_scene(&device, &queue, &mut r, &s, (800, 800), Color::WHITE);
    assert!(pixel(&px, 800, 400, 400)[0] < 4, "interior dark");
    assert!(is_white(pixel(&px, 800, 40, 40)), "far outside white");
    let edge = pixel(&px, 800, 100, 400)[0];
    assert!((100..160).contains(&edge), "the edge is half covered: {edge}");
    let stats = r.last_frame_stats();
    assert_eq!(stats.unsupported[pebbles_gpu::Unsupported::BlurMaskTooLarge as usize], 0);
}

// ---- L63: color matrix ----

#[test]
#[ignore = "gpu"]
fn grayscale_matrix_equalizes_channels() {
    let Some(px) = render("grayscale_matrix_equalizes_channels", (64, 64), |p| {
        effect(p, pebbles_render::paint::ColorMatrix::grayscale().into(), |p| {
            p.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgba8(255, 0, 0, 255),
                None,
                &Rect::new(0.0, 0.0, 64.0, 64.0),
            );
        })
    }) else {
        return;
    };
    let c = pixel(&px, 64, 32, 32);
    assert!(c[..3].iter().all(|&v| (i32::from(v) - 54).abs() <= 1), "{c:?}");
}

#[test]
#[ignore = "gpu"]
fn alpha_offset_reveals_transparent_pixels() {
    // a′ = 0·a + 1: every pixel of the layer becomes opaque (black where it was transparent).
    let mut m = pebbles_render::paint::ColorMatrix::identity().0;
    m[18] = 0.0;
    m[19] = 1.0;
    let Some(px) = render("alpha_offset_reveals_transparent_pixels", (64, 64), |p| {
        effect(p, LayerEffect::ColorMatrix(m), |p| {
            p.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgba8(255, 0, 0, 255),
                None,
                &Rect::new(0.0, 0.0, 32.0, 64.0),
            );
        })
    }) else {
        return;
    };
    assert_eq!(pixel(&px, 64, 16, 32), [255, 0, 0, 255]);
    assert_eq!(pixel(&px, 64, 48, 32), [0, 0, 0, 255], "the transparent half becomes opaque black");
}

// ---- L64: morphology and chains ----

fn square_bounds(px: &[u8]) -> (u32, u32, u32, u32) {
    let dark: Vec<(u32, u32)> = (0..64u32)
        .flat_map(|y| (0..64u32).map(move |x| (x, y)))
        .filter(|&(x, y)| pixel(px, 64, x, y)[0] < 128)
        .collect();
    let xs = dark.iter().map(|p| p.0);
    let ys = dark.iter().map(|p| p.1);
    (xs.clone().min().unwrap(), ys.clone().min().unwrap(), xs.max().unwrap() + 1, ys.max().unwrap() + 1)
}

#[test]
#[ignore = "gpu"]
fn dilate_grows_a_square() {
    let Some(px) = render("dilate_grows_a_square", (64, 64), |p| {
        effect(p, LayerEffect::Dilate { radius_x: 3.0, radius_y: 3.0 }, |p| {
            p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &Rect::new(27.0, 27.0, 37.0, 37.0));
        })
    }) else {
        return;
    };
    assert_eq!(square_bounds(&px), (24, 24, 40, 40), "10×10 dilated by 3 → 16×16");
}

#[test]
#[ignore = "gpu"]
fn erode_shrinks() {
    let Some(px) = render("erode_shrinks", (64, 64), |p| {
        effect(p, LayerEffect::Erode { radius_x: 3.0, radius_y: 3.0 }, |p| {
            p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &Rect::new(22.0, 22.0, 42.0, 42.0));
        })
    }) else {
        return;
    };
    assert_eq!(square_bounds(&px), (25, 25, 39, 39), "20×20 eroded by 3 → 14×14");
}

#[test]
#[ignore = "gpu"]
fn blur_then_matrix_differs_from_matrix_then_blur() {
    let draw = |p: &mut Painter<'_>| {
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(255, 0, 0, 255),
            None,
            &Rect::new(16.0, 16.0, 48.0, 48.0),
        );
    };
    // An alpha contrast (a′ = 2a − 0.5) sharpens a blurred edge but leaves an opaque square
    // opaque, so the two orders differ.
    let mut m = pebbles_render::paint::ColorMatrix::identity().0;
    m[18] = 2.0;
    m[19] = -0.5;
    let inv = LayerEffect::ColorMatrix(m);
    let b = blur(3.0, BlurEdge::Decal);
    let Some(a) = render("blur_then_matrix", (64, 64), |p| {
        effect(p, LayerEffect::chain([b.clone(), inv.clone()]), draw)
    }) else {
        return;
    };
    let Some(c) = render("matrix_then_blur", (64, 64), |p| effect(p, LayerEffect::chain([inv, b]), draw))
    else {
        return;
    };
    assert!(a != c, "effect order matters");
}
