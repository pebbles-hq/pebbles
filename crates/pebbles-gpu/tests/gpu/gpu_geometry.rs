//! R10: Tier A analytic shapes on the GPU — exact edges, fractional coverage, circle profile, and
//! agreement with the tessellated tier.

use super::*;

const W: u32 = 64;

fn draw(
    renderer: &mut Renderer,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    f: impl FnOnce(&mut Painter<'_>),
) -> Vec<u8> {
    let mut scene = Scene::new();
    f(&mut Painter::new(&mut scene));
    render_scene(device, queue, renderer, &scene, (W, W), Color::WHITE)
}

fn black(p: &mut Painter<'_>, shape: &impl kurbo::Shape) {
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, shape);
}

#[test]
#[ignore = "gpu"]
fn sdf_fractional_edge_is_half_covered() {
    let Some((device, queue, mut r)) = setup("sdf_fractional_edge_is_half_covered") else { return };
    let px = draw(&mut r, &device, &queue, |p| black(p, &Rect::new(10.5, 8.0, 30.0, 56.0)));
    for y in 9..=54 {
        let c = pixel(&px, W, 10, y);
        assert!((0..3).all(|i| (125..=130).contains(&c[i])), "column 10 row {y}: {c:?}");
    }
}

#[test]
#[ignore = "gpu"]
fn sdf_integer_rect_is_exact() {
    let Some((device, queue, mut r)) = setup("sdf_integer_rect_is_exact") else { return };
    let px = draw(&mut r, &device, &queue, |p| black(p, &Rect::new(8.0, 8.0, 40.0, 40.0)));
    for y in 0..W {
        for x in 0..W {
            let inside = (8..40).contains(&x) && (8..40).contains(&y);
            let want = if inside { [0, 0, 0, 255] } else { [255, 255, 255, 255] };
            assert_eq!(pixel(&px, W, x, y), want, "({x},{y})");
        }
    }
}

#[test]
#[ignore = "gpu"]
fn sdf_circle_edge_profile() {
    let Some((device, queue, mut r)) = setup("sdf_circle_edge_profile") else { return };
    let px = draw(&mut r, &device, &queue, |p| black(p, &kurbo::Circle::new((32.0, 32.0), 20.0)));
    // Along the +x axis through the centre: pixel centres at 32.5 + k.
    assert_eq!(pixel(&px, W, 49, 32)[0], 0, "distance 17.5: inside");
    assert_eq!(pixel(&px, W, 54, 32)[0], 255, "distance 22.5: outside");
    // Every pixel whose centre is within 0.1 px of the circle is about half covered.
    let mut edges = 0;
    for y in 0..W {
        for x in 0..W {
            let d = (f64::from(x) + 0.5 - 32.0).hypot(f64::from(y) + 0.5 - 32.0);
            if (d - 20.0).abs() <= 0.1 {
                let c = pixel(&px, W, x, y)[0];
                assert!((100..=156).contains(&c), "edge pixel ({x},{y}) at distance {d:.3}: {c}");
                edges += 1;
            }
        }
    }
    assert!(edges > 0, "no pixel centre within 0.1 px of the circle");
}

#[test]
#[ignore = "gpu"]
fn sdf_matches_tessellated_within_edge_tolerance() {
    let Some((device, queue, mut r)) = setup("sdf_matches_tessellated_within_edge_tolerance") else { return };
    let card = kurbo::RoundedRect::new(6.0, 10.0, 58.0, 54.0, 8.0);
    let sdf = draw(&mut r, &device, &queue, |p| black(p, &card));
    r.set_sdf_enabled(false);
    let tess = draw(&mut r, &device, &queue, |p| black(p, &card));
    let mut edge_max = 0u8;
    for y in 0..W {
        for x in 0..W {
            let (a, b) = (pixel(&sdf, W, x, y), pixel(&tess, W, x, y));
            let delta = (0..3).map(|i| a[i].abs_diff(b[i])).max().unwrap_or(0);
            let interior = (14..50).contains(&x) && (18..46).contains(&y);
            if interior {
                assert_eq!(a, b, "interior ({x},{y})");
            }
            edge_max = edge_max.max(delta);
        }
    }
    assert!(edge_max <= 64, "max edge delta {edge_max}");
}
