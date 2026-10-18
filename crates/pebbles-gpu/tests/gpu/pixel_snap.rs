//! R18 pixel-grid policy (02 §5.3) and R19 exact box coverage / texel-exact images.

use pebbles_render::paint::display_list::PixelSnap;

use super::*;

const W: u32 = 96;

/// `scene` under a uniform device scale `s` (as the runner appends the logical scene).
fn scaled(scene: &Scene, s: f64) -> Scene {
    let mut frame = Scene::new();
    frame.append(scene, Some(Affine::scale(s)));
    frame
}

/// AC1 (R18): a 1-logical-px border renders as `round(s)` fully opaque device columns with no
/// partially covered neighbor, at every desktop scale factor.
#[test]
#[ignore = "gpu"]
fn pixel_snap_border_is_whole_columns() {
    let Some((device, queue, mut r)) = setup("pixel_snap_border_is_whole_columns") else { return };
    for s in [1.0, 1.25, 1.5, 1.75, 2.0] {
        let mut scene = Scene::new();
        let style = Stroke::new(1.0).with_join(kurbo::Join::Miter);
        Painter::new(&mut scene).stroke(
            &style,
            Affine::IDENTITY,
            Color::BLACK,
            None,
            &Rect::new(10.3, 10.3, 40.7, 40.7),
        );
        let px = render_scene(&device, &queue, &mut r, &scaled(&scene, s), (W, W), Color::WHITE);
        let row = (25.0 * s) as u32;
        let dark: Vec<u32> = (0..W).filter(|&x| pixel(&px, W, x, row)[0] == 0).collect();
        let partial: Vec<u32> = (0..W).filter(|&x| !matches!(pixel(&px, W, x, row)[0], 0 | 255)).collect();
        let want = (s + 0.5f64).floor() as usize;
        assert!(partial.is_empty(), "scale {s}: partial columns {partial:?}");
        assert_eq!(dark.len(), 2 * want, "scale {s}: dark columns {dark:?}");
    }
}

/// AC2 (R18): a rect moving 0.1 px per frame moves smoothly with `Off` and stays on the grid
/// with `Auto`.
#[test]
#[ignore = "gpu"]
fn pixel_snap_animation_off_is_smooth_auto_is_on_grid() {
    let Some((device, queue, mut r)) = setup("pixel_snap_animation") else { return };
    let edge = |r: &mut Renderer, snap: PixelSnap, x: f64| {
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        p.set_pixel_snap(snap);
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &Rect::new(x, 10.0, x + 30.0, 40.0));
        let px = render_scene(&device, &queue, r, &scene, (W, W), Color::WHITE);
        pixel(&px, W, 10, 20)[0]
    };
    let off: Vec<u8> = (0..8).map(|k| edge(&mut r, PixelSnap::Off, 10.0 + 0.1 * f64::from(k))).collect();
    assert!(off.windows(2).all(|w| w[1] > w[0]), "Off: the edge pixel lightens every frame {off:?}");
    let auto: Vec<u8> = (0..8).map(|k| edge(&mut r, PixelSnap::Auto, 10.0 + 0.1 * f64::from(k))).collect();
    assert!(auto.iter().all(|v| matches!(v, 0 | 255)), "Auto: on the grid {auto:?}");
}

/// R19 AC1: the rect [10.25, 20.75] × [5.5, 8.0] (no snapping) is exactly the product of the
/// 1-D overlaps, quantized once (Δ = 0). Where the exact value is a quantization tie (x.5, e.g.
/// coverage 0.5 → 127.5) the fixed-function blend's tie rounding decides, so Δ ≤ 1 there.
#[test]
#[ignore = "gpu"]
fn exact_rect_matches_box_coverage() {
    let Some((device, queue, mut r)) = setup("exact_rect_matches_box_coverage") else { return };
    let rect = Rect::new(10.25, 5.5, 20.75, 8.0);
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.set_pixel_snap(PixelSnap::Off);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &rect);
    let px = render_scene(&device, &queue, &mut r, &scene, (32, 16), Color::WHITE);
    let overlap = |a: f64, b: f64, i: u32| (b.min(f64::from(i) + 1.0) - a.max(f64::from(i))).clamp(0.0, 1.0);
    for y in 0..16 {
        for x in 0..32 {
            let cov = overlap(rect.x0, rect.x1, x) * overlap(rect.y0, rect.y1, y);
            let exact = (1.0 - cov) * 255.0;
            let got = f64::from(pixel(&px, 32, x, y)[0]);
            let tie = exact.fract() == 0.5;
            let ok = if tie { (got - exact).abs() == 0.5 } else { got == (exact + 0.5).floor() };
            assert!(ok, "({x},{y}) coverage {cov}: got {got}, exact {exact}");
        }
    }
}

/// R19 AC2: a 16×16 bitmap at a fractional position (scale 1) is byte-identical to its
/// texels after snapping the origin.
#[test]
#[ignore = "gpu"]
fn texel_exact_image_at_fractional_position() {
    let Some((device, queue, mut r)) = setup("texel_exact_image_at_fractional_position") else { return };
    let texels: Vec<u8> = (0..256u32)
        .flat_map(|i| [(i * 37 % 256) as u8, (i * 91 % 256) as u8, (i * 13 % 256) as u8, 255])
        .collect();
    let image = peniko::ImageData {
        data: peniko::Blob::from(texels.clone()),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::Alpha,
        width: 16,
        height: 16,
    };
    let px = render_scene(
        &device,
        &queue,
        &mut r,
        &image_scene(&image, Affine::translate((3.3, 4.6))),
        (32, 32),
        Color::WHITE,
    );
    // Origin (3.3, 4.6) rounds half up to (3, 5).
    for ty in 0..16 {
        for tx in 0..16 {
            let i = ((ty * 16 + tx) * 4) as usize;
            assert_eq!(
                pixel(&px, 32, 3 + tx, 5 + ty),
                [texels[i], texels[i + 1], texels[i + 2], 255],
                "texel ({tx},{ty})"
            );
        }
    }
}
