//! Box shadows (02 §8.2, R40): the closed form in local space.

use super::*;

/// A black opaque shadow of `Rect(50,50,150,150)`, σ 10, radius 0, under `t`, on white 200×200.
fn shadow_px(r: &mut (wgpu::Device, wgpu::Queue, Renderer), t: Affine) -> Vec<u8> {
    let mut s = Scene::new();
    Painter::new(&mut s).draw_blurred_rounded_rect(
        t,
        Rect::new(50.0, 50.0, 150.0, 150.0),
        Color::BLACK,
        0.0,
        10.0,
    );
    render_scene(&r.0, &r.1, &mut r.2, &s, (200, 200), Color::WHITE)
}

fn rgb_in(p: [u8; 4], lo: u8, hi: u8) -> bool {
    p[..3].iter().all(|&c| (lo..=hi).contains(&c))
}

#[test]
#[ignore = "gpu"]
fn shadow_center_is_full() {
    let Some(mut r) = setup("shadow_center_is_full") else { return };
    let p = pixel(&shadow_px(&mut r, Affine::IDENTITY), 200, 100, 100);
    assert!(rgb_in(p, 0, 1), "{p:?}");
}

#[test]
#[ignore = "gpu"]
fn shadow_edge_midpoint_is_half() {
    let Some(mut r) = setup("shadow_edge_midpoint_is_half") else { return };
    // The edge y = 150 lies between pixel rows 149 and 150: their mean is the edge value (½).
    let px = shadow_px(&mut r, Affine::IDENTITY);
    let mean = (u16::from(pixel(&px, 200, 100, 149)[0]) + u16::from(pixel(&px, 200, 100, 150)[0])) / 2;
    assert!((124..=131).contains(&mean), "edge mean {mean}");
}

#[test]
#[ignore = "gpu"]
fn shadow_fades_by_2_5_sigma() {
    let Some(mut r) = setup("shadow_fades_by_2_5_sigma") else { return };
    let p = pixel(&shadow_px(&mut r, Affine::IDENTITY), 200, 100, 175);
    assert!(rgb_in(p, 253, 255), "{p:?}");
}

#[test]
#[ignore = "gpu"]
fn rotated_shadow_is_rotated() {
    let Some(mut r) = setup("rotated_shadow_is_rotated") else { return };
    let t = Affine::rotate_about(std::f64::consts::FRAC_PI_4, kurbo::Point::new(100.0, 100.0));
    let px = shadow_px(&mut r, t);
    // Inside the axis-aligned bbox of the rotated square (x, y ≥ 29.3) but ≈ 2.6σ outside the
    // square itself (along the diagonal: 75.7 − 50 = 25.7 px).
    let p = pixel(&px, 200, 46, 46);
    assert!(rgb_in(p, 250, 255), "{p:?} (an AABB shader would darken it)");
    assert!(rgb_in(pixel(&px, 200, 100, 100), 0, 1));
}

#[test]
#[ignore = "gpu"]
fn card_shadow_does_not_cover_card() {
    let Some((device, queue, mut r)) = setup("card_shadow_does_not_cover_card") else { return };
    let card = Rect::new(50.0, 50.0, 150.0, 150.0);
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    p.draw_blurred_rounded_rect(
        Affine::translate((0.0, 4.0)),
        card,
        Color::from_rgba8(0, 0, 0, 100),
        12.0,
        8.0,
    );
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &card.to_rounded_rect(12.0));
    let px = render_scene(&device, &queue, &mut r, &s, (200, 200), Color::from_rgba8(0xF0, 0xF0, 0xF0, 0xFF));
    assert_eq!(pixel(&px, 200, 100, 100), [255, 255, 255, 255]);
}

/// R41: σ = 0 is a hard-edged rect: 1 px inside the edge is black, 1 px outside is white.
#[test]
#[ignore = "gpu"]
fn zero_sigma_is_hard_edge() {
    let Some(mut r) = setup("zero_sigma_is_hard_edge") else { return };
    let mut s = Scene::new();
    Painter::new(&mut s).draw_blurred_rounded_rect(
        Affine::IDENTITY,
        Rect::new(50.0, 50.0, 150.0, 150.0),
        Color::BLACK,
        0.0,
        0.0,
    );
    let px = render_scene(&r.0, &r.1, &mut r.2, &s, (200, 200), Color::WHITE);
    assert!(rgb_in(pixel(&px, 200, 100, 148), 0, 1), "{:?}", pixel(&px, 200, 100, 148));
    assert!(rgb_in(pixel(&px, 200, 100, 151), 254, 255), "{:?}", pixel(&px, 200, 100, 151));
}

/// R42: a blurred star mask. Normal: smooth falloff outside the star (no hard edge); Solid: the
/// interior stays solid away from the edges.
#[test]
#[ignore = "gpu"]
fn blurred_star_shadow() {
    use pebbles_render::paint::display_list::BlurStyle;
    let Some(mut r) = setup("blurred_star_shadow") else { return };
    let mut star = kurbo::BezPath::new();
    for i in 0..10 {
        let a = std::f64::consts::PI * f64::from(i) / 5.0 - std::f64::consts::FRAC_PI_2;
        let rad = if i % 2 == 0 { 40.0 } else { 17.0 };
        let p = (64.0 + rad * a.cos(), 64.0 + rad * a.sin());
        if i == 0 { star.move_to(p) } else { star.line_to(p) }
    }
    star.close_path();
    let draw = |r: &mut (wgpu::Device, wgpu::Queue, Renderer), style| {
        let mut s = Scene::new();
        Painter::new(&mut s).fill_blurred(Fill::NonZero, Affine::IDENTITY, Color::BLACK, &star, 4.0, style);
        render_scene(&r.0, &r.1, &mut r.2, &s, (128, 128), Color::WHITE)
    };
    let normal = draw(&mut r, BlurStyle::Normal);
    // Along the ray straight up from the tip (64, 24): strictly increasing brightness outward.
    let col: Vec<u8> = (8..24).map(|y| pixel(&normal, 128, 64, y)[0]).collect();
    assert!(col.windows(2).all(|w| w[0] >= w[1]), "not a smooth falloff: {col:?}");
    assert!(col[0] > 240 && *col.last().unwrap() < 240, "{col:?}");
    let solid = draw(&mut r, BlurStyle::Solid);
    let c = pixel(&solid, 128, 64, 64);
    assert!(c[0] <= 2, "solid interior {c:?}");
}
