//! Blend and compose layers (09 §4, L21–L25) against the W3C CPU reference (`blend_ref`).

use super::blend_ref::{composite, to_u8};
use super::*;
use peniko::{BlendMode, Compose, Mix};

const COMPOSES: [Compose; 14] = [
    Compose::Clear,
    Compose::Copy,
    Compose::Dest,
    Compose::SrcOver,
    Compose::DestOver,
    Compose::SrcIn,
    Compose::DestIn,
    Compose::SrcOut,
    Compose::DestOut,
    Compose::SrcAtop,
    Compose::DestAtop,
    Compose::Xor,
    Compose::Plus,
    Compose::PlusLighter,
];

const MIXES: [Mix; 16] = [
    Mix::Normal,
    Mix::Multiply,
    Mix::Screen,
    Mix::Overlay,
    Mix::Darken,
    Mix::Lighten,
    Mix::ColorDodge,
    Mix::ColorBurn,
    Mix::HardLight,
    Mix::SoftLight,
    Mix::Difference,
    Mix::Exclusion,
    Mix::Hue,
    Mix::Saturation,
    Mix::Color,
    Mix::Luminosity,
];

fn render(
    r: &mut (wgpu::Device, wgpu::Queue, Renderer),
    s: &Scene,
    size: (u32, u32),
    base: Color,
) -> Vec<u8> {
    render_scene(&r.0, &r.1, &mut r.2, s, size, base)
}

/// Premultiplied f64 → the straight `Color` that the painter premultiplies back.
fn straight(c: [f64; 4]) -> Color {
    let a = c[3];
    let u = |v: f64| if a > 0.0 { (v / a * 255.0).round() as u8 } else { 0 };
    Color::from_rgba8(u(c[0]), u(c[1]), u(c[2]), (a * 255.0).round() as u8)
}

/// The 8-bit premultiplied value the GPU actually draws for straight `c`.
fn premul8(c: Color) -> [f64; 4] {
    let [r, g, b, a] = c.to_rgba8().to_u8_array();
    let a = f64::from(a) / 255.0;
    [f64::from(r) / 255.0 * a, f64::from(g) / 255.0 * a, f64::from(b) / 255.0 * a, a]
}

fn close(got: [u8; 4], want: [u8; 4], tol: i32) -> bool {
    (0..3).all(|i| (i32::from(got[i]) - i32::from(want[i])).abs() <= tol)
}

fn over_white(c: [f64; 4]) -> [f64; 4] {
    composite([1.0; 4], c, Mix::Normal, Compose::SrcOver)
}

/// L21 AC3: every compose mode with `Mix::Normal` under a scissor-only clip takes the
/// fixed-function path: backdrop `#2060C0`, a `#E04020` α 0.75 circle in the layer.
#[test]
#[ignore = "gpu"]
fn fixed_function_compose_modes() {
    let Some(mut r) = setup("fixed_function_compose_modes") else { return };
    let b_col = Color::from_rgba8(0x20, 0x60, 0xC0, 0xFF);
    let s_col = Color::from_rgba8(0xE0, 0x40, 0x20, 0xBF);
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    for compose in COMPOSES {
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        p.fill(Fill::NonZero, Affine::IDENTITY, b_col, None, &Rect::new(0.0, 0.0, 64.0, 64.0));
        p.push_layer(
            Fill::NonZero,
            BlendMode::new(Mix::Normal, compose),
            1.0,
            Affine::IDENTITY,
            &Rect::new(8.0, 8.0, 56.0, 56.0),
        );
        p.fill(Fill::NonZero, Affine::IDENTITY, s_col, None, &kurbo::Circle::new((32.0, 32.0), 16.0));
        p.pop_layer();
        let px = render(&mut r, &s, (64, 64), Color::WHITE);
        // The layer composites into the root, whose content is the opaque backdrop: the result
        // is the composite itself (Clear really clears the root).
        let want = to_u8(composite(premul8(b_col), premul8(s_col), Mix::Normal, compose));
        let got = pixel(&px, 64, 32, 32);
        assert!(
            close(got, want, 3) && (i32::from(got[3]) - i32::from(want[3])).abs() <= 3,
            "{compose:?}: {got:?} vs {want:?}"
        );
        // Inside the layer bounds but outside the circle the source is transparent.
        let want_edge = to_u8(composite(premul8(b_col), [0.0; 4], Mix::Normal, compose));
        let got_edge = pixel(&px, 64, 10, 10);
        assert!(close(got_edge, want_edge, 3), "{compose:?} empty source: {got_edge:?} vs {want_edge:?}");
        // Outside the layer bounds nothing changes.
        assert!(close(pixel(&px, 64, 2, 2), [0x20, 0x60, 0xC0, 255], 1), "{compose:?} outside");
    }
}

/// L23 AC1/AC2: a Multiply layer over `#808080` with a `#00FF00` fill → (0, 128, 0); the root
/// renders through the intermediate + blit (no MSAA on the root).
#[test]
#[ignore = "gpu"]
fn multiply_backdrop_over_gray() {
    let Some(mut r) = setup("multiply_backdrop_over_gray") else { return };
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    p.push_layer(Fill::NonZero, Mix::Multiply, 1.0, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 64.0));
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0, 255, 0, 255),
        None,
        &Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    p.pop_layer();
    let px = render(&mut r, &s, (64, 64), Color::from_rgba8(0x80, 0x80, 0x80, 0xFF));
    let c = pixel(&px, 64, 32, 32);
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert!(close(c, [0, 128, 0, 255], 2), "{c:?}");
}

/// L23 AC3: a backdrop layer at bounds (16,16)–(32,32) leaves every pixel outside untouched.
#[test]
#[ignore = "gpu"]
fn backdrop_copy_region_is_bounds() {
    let Some(mut r) = setup("backdrop_copy_region_is_bounds") else { return };
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(200, 100, 50, 255),
        None,
        &Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    p.push_layer(Fill::NonZero, Mix::Difference, 1.0, Affine::IDENTITY, &Rect::new(16.0, 16.0, 32.0, 32.0));
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &Rect::new(0.0, 0.0, 64.0, 64.0));
    p.pop_layer();
    let px = render(&mut r, &s, (64, 64), Color::WHITE);
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    for (x, y) in [(15, 20), (32, 20), (20, 15), (20, 32)] {
        assert_eq!(pixel(&px, 64, x, y), [200, 100, 50, 255], "({x},{y})");
    }
    let c = pixel(&px, 64, 24, 24);
    assert!(close(c, [55, 155, 205, 255], 2), "difference inside: {c:?}");
}

/// L23 AC4: a backdrop composite under an SDF circle clip lerps by coverage: interior =
/// composite, outside = backdrop, the edge in between.
#[test]
#[ignore = "gpu"]
fn backdrop_lerps_by_sdf_coverage() {
    let Some(mut r) = setup("backdrop_lerps_by_sdf_coverage") else { return };
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(200, 100, 50, 255),
        None,
        &Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    p.push_layer(
        Fill::NonZero,
        Mix::Difference,
        1.0,
        Affine::IDENTITY,
        &kurbo::Circle::new((32.0, 32.0), 20.0),
    );
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &Rect::new(0.0, 0.0, 64.0, 64.0));
    p.pop_layer();
    let px = render(&mut r, &s, (64, 64), Color::WHITE);
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert!(close(pixel(&px, 64, 32, 32), [55, 155, 205, 255], 2));
    assert_eq!(pixel(&px, 64, 2, 2), [200, 100, 50, 255]);
    // Pixels straddling the circle edge lie strictly between the two.
    let partial = (0..64u32)
        .flat_map(|y| (0..64u32).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let d = ((f64::from(x) + 0.5 - 32.0).powi(2) + (f64::from(y) + 0.5 - 32.0).powi(2)).sqrt();
            (d - 20.0).abs() < 0.3
        })
        .filter(|&(x, y)| {
            let v = pixel(&px, 64, x, y)[0];
            v > 70 && v < 185
        })
        .count();
    assert!(partial > 0, "no partially covered edge pixel");
}

/// L20 AC1 (ColorFiltered over a mixed backdrop): a white box (left) and a `#808080` box
/// (right) under a full-canvas Multiply layer filled `#00FF00`.
#[test]
#[ignore = "gpu"]
fn color_filter_multiply_green() {
    let Some(mut r) = setup("color_filter_multiply_green") else { return };
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &Rect::new(0.0, 0.0, 32.0, 64.0));
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0x80, 0x80, 0x80, 255),
        None,
        &Rect::new(32.0, 0.0, 64.0, 64.0),
    );
    p.push_layer(Fill::NonZero, Mix::Multiply, 1.0, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 64.0));
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0, 255, 0, 255),
        None,
        &Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    p.pop_layer();
    let px = render(&mut r, &s, (64, 64), Color::WHITE);
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert!(close(pixel(&px, 64, 16, 32), [0, 255, 0, 255], 2), "{:?}", pixel(&px, 64, 16, 32));
    assert!(close(pixel(&px, 64, 48, 32), [0, 128, 0, 255], 2), "{:?}", pixel(&px, 64, 48, 32));
}

/// L20 AC2: screen(0.5, 0.5) = 0.75.
#[test]
#[ignore = "gpu"]
fn screen_gray_on_gray() {
    let Some(mut r) = setup("screen_gray_on_gray") else { return };
    let gray = Color::from_rgba8(0x80, 0x80, 0x80, 255);
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    p.fill(Fill::NonZero, Affine::IDENTITY, gray, None, &Rect::new(0.0, 0.0, 64.0, 64.0));
    p.push_layer(Fill::NonZero, Mix::Screen, 1.0, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 64.0));
    p.fill(Fill::NonZero, Affine::IDENTITY, gray, None, &Rect::new(0.0, 0.0, 64.0, 64.0));
    p.pop_layer();
    let px = render(&mut r, &s, (64, 64), Color::WHITE);
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert!(close(pixel(&px, 64, 32, 32), [192, 192, 192, 255], 2), "{:?}", pixel(&px, 64, 32, 32));
}

/// L20 AC4 (§3.1): blue base; a pure rect clip whose layer is promoted by its Multiply child;
/// gray over the left half; the Multiply red layer sees only the clip layer's content.
#[test]
#[ignore = "gpu"]
fn blend_inside_pure_clip_uses_clip_layer_backdrop() {
    let Some(mut r) = setup("blend_inside_pure_clip_uses_clip_layer_backdrop") else { return };
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 64.0));
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0x80, 0x80, 0x80, 255),
        None,
        &Rect::new(0.0, 0.0, 32.0, 64.0),
    );
    p.push_layer(Fill::NonZero, Mix::Multiply, 1.0, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 64.0));
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(255, 0, 0, 255),
        None,
        &Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    p.pop_layer();
    p.pop_layer();
    let px = render(&mut r, &s, (64, 64), Color::from_rgba8(0, 0, 255, 255));
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert!(close(pixel(&px, 64, 16, 32), [128, 0, 0, 255], 2), "{:?}", pixel(&px, 64, 16, 32));
    assert!(close(pixel(&px, 64, 48, 32), [255, 0, 0, 255], 2), "not black: {:?}", pixel(&px, 64, 48, 32));
}

/// L25: every Mix × Compose × (B, S) pair against the CPU reference, ±3 per channel. Each
/// block is a promoted layer holding `B` and a `(mix, compose)` layer with `S`.
#[test]
#[ignore = "gpu"]
fn blend_matrix_matches_cpu_reference() {
    let Some(mut r) = setup("blend_matrix_matches_cpu_reference") else { return };
    let bs = [[0.8, 0.4, 0.2, 1.0], [0.15, 0.3, 0.45, 0.5]];
    let ss = [[0.2, 0.5, 0.7, 1.0], [0.54, 0.12, 0.24, 0.6]];
    const BLOCK: f64 = 4.0;
    let pairs: Vec<(usize, usize)> = (0..2).flat_map(|b| (0..2).map(move |s| (b, s))).collect();
    let mut failures = Vec::new();
    for mix in MIXES {
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        for (ci, compose) in COMPOSES.iter().enumerate() {
            for (pi, &(bi, si)) in pairs.iter().enumerate() {
                let x = ci as f64 * BLOCK;
                let y = pi as f64 * BLOCK;
                let block = Rect::new(x, y, x + BLOCK, y + BLOCK);
                p.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &block);
                p.fill(Fill::NonZero, Affine::IDENTITY, straight(bs[bi]), None, &block);
                p.push_layer(Fill::NonZero, BlendMode::new(mix, *compose), 1.0, Affine::IDENTITY, &block);
                p.fill(Fill::NonZero, Affine::IDENTITY, straight(ss[si]), None, &block);
                p.pop_layer();
                p.pop_layer();
            }
        }
        let w = (COMPOSES.len() as f64 * BLOCK) as u32;
        let h = (pairs.len() as f64 * BLOCK) as u32;
        let px = render(&mut r, &s, (w, h), Color::WHITE);
        for (ci, compose) in COMPOSES.iter().enumerate() {
            for (pi, &(bi, si)) in pairs.iter().enumerate() {
                let b = premul8(straight(bs[bi]));
                let sc = premul8(straight(ss[si]));
                let want = to_u8(over_white(composite(b, sc, mix, *compose)));
                let (cx, cy) = (ci as u32 * 4 + 2, pi as u32 * 4 + 2);
                let got = pixel(&px, w, cx, cy);
                if !close(got, want, 3) {
                    failures.push(format!("{mix:?} {compose:?} pair (B{bi}, S{si}): {got:?} vs {want:?}"));
                }
            }
        }
    }
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert!(failures.is_empty(), "{} mismatches:\n{}", failures.len(), failures.join("\n"));
}

/// L20 AC3 (ShaderMask): red box; an outer Normal layer; inside it a `(Normal, DestIn)` layer
/// filled with a transparent-white → opaque-white gradient over x 0..64.
#[test]
#[ignore = "gpu"]
fn shader_mask_dest_in_gradient() {
    let Some(mut r) = setup("shader_mask_dest_in_gradient") else { return };
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    let all = Rect::new(0.0, 0.0, 64.0, 64.0);
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &all);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgba8(255, 0, 0, 255), None, &all);
    p.push_layer(Fill::NonZero, BlendMode::new(Mix::Normal, Compose::DestIn), 1.0, Affine::IDENTITY, &all);
    let g = peniko::Gradient::new_linear((0.0, 0.0), (64.0, 0.0))
        .with_stops([Color::from_rgba8(255, 255, 255, 0), Color::WHITE].as_slice());
    p.fill(Fill::NonZero, Affine::IDENTITY, &g, None, &all);
    p.pop_layer();
    p.pop_layer();
    let px = render(&mut r, &s, (64, 64), Color::WHITE);
    // Pixel 0's center is at t = 0.5/64 (exact mask 0.008 → ≥ 253); pixel 1 is already at
    // t = 0.023 (exactly 249), so the ≥ 250 probe sits on column 0.
    let l = pixel(&px, 64, 0, 32);
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert!(l[..3].iter().all(|&v| v >= 250), "left fades to white: {l:?}");
    // Column 63 (t = 63.5/64: exactly (255, 2, 2)); column 62 is already (255, 6, 6).
    let r = pixel(&px, 64, 63, 32);
    assert!(close(r, [255, 0, 0, 255], 4), "right keeps red: {r:?}");
}
