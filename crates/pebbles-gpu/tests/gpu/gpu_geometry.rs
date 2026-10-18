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
    let px = draw(&mut r, &device, &queue, |p| {
        // Exact coverage, not the pixel-grid policy (R18 snaps this edge under `Auto`).
        p.set_pixel_snap(pebbles_render::paint::display_list::PixelSnap::Off);
        black(p, &Rect::new(10.5, 8.0, 30.0, 56.0));
    });
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
    // The tessellated reference is MSAA-4: its own edge error is one sample quantum (64). The
    // exact check is the golden model (render-check prim/rrect_*); here: quantum + the 2-LSB
    // Tier A curve budget (02 §5.2).
    assert!(edge_max <= 64 + 2, "max edge delta {edge_max}");
}

// ---- R11: analytic strokes ----

fn stroke(p: &mut Painter<'_>, width: f64, shape: &impl kurbo::Shape) {
    p.stroke(&Stroke::new(width), Affine::IDENTITY, Color::BLACK, None, shape);
}

#[test]
#[ignore = "gpu"]
fn sdf_one_px_border_is_crisp() {
    let Some((device, queue, mut r)) = setup("sdf_one_px_border_is_crisp") else { return };
    let px = draw(&mut r, &device, &queue, |p| stroke(p, 1.0, &Rect::new(10.5, 10.5, 50.5, 50.5)));
    for y in 12..48 {
        assert_eq!(pixel(&px, W, 10, y), [0, 0, 0, 255], "column 10 row {y}");
        assert_eq!(pixel(&px, W, 9, y), [255, 255, 255, 255], "column 9 row {y}");
        assert_eq!(pixel(&px, W, 11, y), [255, 255, 255, 255], "column 11 row {y}");
    }
}

/// Signed distance to a rounded rect (f64 CPU mirror of sdf.wgsl's `sd_rrect`, uniform radius).
fn sd_rrect(p: (f64, f64), r: Rect, radius: f64) -> f64 {
    let (cx, cy) = ((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0);
    let (hx, hy) = (r.width() / 2.0, r.height() / 2.0);
    let (qx, qy) = ((p.0 - cx).abs() - hx + radius, (p.1 - cy).abs() - hy + radius);
    qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0) - radius
}

/// Exact (16×16 supersampled) white-on-black value of a pixel for the 2 px border of `card`.
fn exact_border_value(card: kurbo::RoundedRect, x: u32, y: u32) -> u8 {
    let (outer, inner) = (card.rect().inflate(1.0, 1.0), card.rect().inflate(-1.0, -1.0));
    let r = card.radii().top_left;
    let mut hits = 0u32;
    for i in 0..16 {
        for j in 0..16 {
            let p = (f64::from(x) + (f64::from(i) + 0.5) / 16.0, f64::from(y) + (f64::from(j) + 0.5) / 16.0);
            hits += u32::from(sd_rrect(p, outer, r + 1.0) <= 0.0 && sd_rrect(p, inner, r - 1.0) > 0.0);
        }
    }
    u8::try_from(255 - (255 * hits + 128) / 256).unwrap_or(0)
}

fn card() -> kurbo::RoundedRect {
    kurbo::RoundedRect::new(6.0, 10.0, 58.0, 54.0, 8.0)
}

/// R11: the analytic border is within 16 of exact area coverage everywhere.
#[test]
#[ignore = "gpu"]
fn sdf_border_matches_exact_coverage() {
    let Some((device, queue, mut r)) = setup("sdf_border_matches_exact_coverage") else { return };
    let sdf = draw(&mut r, &device, &queue, |p| stroke(p, 2.0, &card()));
    for y in 0..W {
        for x in 0..W {
            let (got, want) = (pixel(&sdf, W, x, y)[0], exact_border_value(card(), x, y));
            assert!(got.abs_diff(want) <= 16, "({x},{y}) sdf {got} exact {want}");
        }
    }
}

/// R11 (02 card): SDF vs tessellated border — identical away from edges, ≤ 64 on edge pixels.
/// Pixels where the tessellated (4× MSAA) reference is itself more than one MSAA step (32) from
/// exact coverage are excluded and counted: there the reference is the outlier (e.g. pixel
/// (54,10): exact 55, SDF 63, tessellated 128).
#[test]
#[ignore = "gpu"]
fn sdf_border_matches_tessellated() {
    let Some((device, queue, mut r)) = setup("sdf_border_matches_tessellated") else { return };
    let sdf = draw(&mut r, &device, &queue, |p| stroke(p, 2.0, &card()));
    r.set_sdf_enabled(false);
    let tess = draw(&mut r, &device, &queue, |p| stroke(p, 2.0, &card()));
    let partial = |c: [u8; 4]| c[..3].iter().any(|v| *v != 0 && *v != 255);
    let (mut edge_max, mut reference_outliers) = (0u8, 0u32);
    for y in 0..W {
        for x in 0..W {
            let (a, b) = (pixel(&sdf, W, x, y), pixel(&tess, W, x, y));
            if !partial(a) && !partial(b) {
                assert_eq!(a, b, "away from the border ({x},{y})");
                continue;
            }
            let exact = exact_border_value(card(), x, y);
            if b[0].abs_diff(exact) > 32 {
                // Excluded only where the analytic border is the closer one to exact coverage.
                assert!(
                    a[0].abs_diff(exact) < b[0].abs_diff(exact),
                    "({x},{y}) sdf {} tess {} exact {exact}",
                    a[0],
                    b[0]
                );
                reference_outliers += 1;
                continue;
            }
            edge_max = edge_max.max((0..3).map(|i| a[i].abs_diff(b[i])).max().unwrap_or(0));
        }
    }
    // The tessellated reference is MSAA-4: its own edge error is one sample quantum (64). The
    // exact check is the golden model (render-check prim/rrect_*); here: quantum + the 2-LSB
    // Tier A curve budget (02 §5.2).
    assert!(edge_max <= 64 + 2, "max edge delta {edge_max}");
    eprintln!(
        "sdf_border_matches_tessellated: {reference_outliers} pixels where the MSAA reference is the outlier"
    );
}

#[test]
#[ignore = "gpu"]
fn sdf_ring() {
    let Some((device, queue, mut r)) = setup("sdf_ring") else { return };
    let px = draw(&mut r, &device, &queue, |p| stroke(p, 4.0, &kurbo::Circle::new((32.0, 32.0), 20.0)));
    // Along +x from the centre (32, 32): pixel x covers distances [x - 32, x - 31].
    assert_eq!(pixel(&px, W, 51, 31)[0], 0, "distance ~20: on the ring");
    assert_eq!(pixel(&px, W, 47, 31)[0], 255, "distance ~16: inside the ring");
    assert_eq!(pixel(&px, W, 55, 31)[0], 255, "distance ~24: outside");
}
