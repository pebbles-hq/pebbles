//! M07: a frame cut into many segments (tiny arena caps) renders identically to one segment.

use pebbles_gpu::testing::bench_scenes::BENCH_CASES;

use super::*;

fn render_case(name: &str, budget: Option<u64>) -> (Vec<u8>, pebbles_gpu::FrameStats) {
    let (_, size, f) = BENCH_CASES.iter().find(|(n, ..)| *n == name).expect("case");
    let setup = setup_with_budgets(name, |b| {
        if let Some(bytes) = budget {
            b.arena_max_bytes = bytes;
        }
    });
    let Some((device, queue, mut renderer)) = setup else { return (Vec::new(), Default::default()) };
    let mut scene = Scene::new();
    f(&mut Painter::new(&mut scene));
    let px = render_scene(&device, &queue, &mut renderer, &scene, *size, Color::WHITE);
    (px, renderer.last_frame_stats())
}

#[test]
#[ignore = "gpu"]
fn segmented_frame_matches_single_segment() {
    // (case, cap): small enough to cut each scene into several segments.
    let cases = [
        ("bench/rects_10k", 4096),
        ("bench/paths_2k", 4096),
        ("bench/dashboard_1200x800", 256),
        ("bench/glyph_runs_1k", 4096),
        ("bench/images_64", 1024),
        ("bench/layers_100", 512),
        ("bench/gradients_500", 1024),
        ("bench/shadows_200", 1024),
    ];
    for (name, cap) in cases {
        let (one, s1) = render_case(name, None);
        if one.is_empty() {
            return;
        }
        let (many, sn) = render_case(name, Some(cap));
        assert_eq!(s1.segments, 1, "{name}: default budgets are one segment");
        assert!(sn.segments > 1, "{name}: {} segments at a {cap} B cap", sn.segments);
        assert_eq!(sn.unsupported[pebbles_gpu::Unsupported::GeometryTooLarge as usize], 0, "{name}");
        let max = one.iter().zip(&many).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
        assert_eq!(max, 0, "{name}: max channel delta across {} segments", sn.segments);
    }
}

/// GPU mask jobs (volatile clips) and Tier C runs (volatile paths) across segments.
#[test]
#[ignore = "gpu"]
fn segmented_masks_and_tier_c_match() {
    let scene = || {
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        p.set_path_hint(pebbles_render::paint::display_list::PathHint::Volatile);
        for i in 0..12u32 {
            let (x, y) = (f64::from(i % 4) * 60.0, f64::from(i / 4) * 60.0);
            let mut star = kurbo::BezPath::new();
            for k in 0..10 {
                let r = if k % 2 == 0 { 28.0 } else { 12.0 };
                let a = std::f64::consts::PI * f64::from(k) / 5.0;
                let pt = (x + 30.0 + r * a.sin(), y + 30.0 - r * a.cos());
                if k == 0 {
                    star.move_to(pt);
                } else {
                    star.line_to(pt);
                }
            }
            star.close_path();
            p.push_layer(Fill::NonZero, peniko::Mix::Normal, 1.0, Affine::IDENTITY, &star);
            for j in 0..6u32 {
                let c = Color::from_rgb8((i * 20) as u8, (j * 40) as u8, 120);
                p.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    c,
                    None,
                    &Rect::new(x, y + f64::from(j) * 10.0, x + 60.0, y + f64::from(j) * 10.0 + 10.0),
                );
            }
            p.pop_layer();
            // A volatile path: a Tier C run.
            p.fill(Fill::NonZero, Affine::translate((4.0, 4.0)), Color::from_rgba8(0, 0, 0, 90), None, &star);
        }
        s
    };
    let render = |cap: Option<u64>| {
        let setup = setup_with_budgets("segmented_masks_and_tier_c_match", |b| {
            if let Some(c) = cap {
                b.arena_max_bytes = c;
            }
        });
        let Some((device, queue, mut renderer)) = setup else { return (Vec::new(), Default::default()) };
        let px = render_scene(&device, &queue, &mut renderer, &scene(), (240, 180), Color::WHITE);
        (px, renderer.last_frame_stats())
    };
    let (one, s1) = render(None);
    if one.is_empty() {
        return;
    }
    let (many, sn) = render(Some(2048));
    assert!(s1.gpu_masks > 0 && s1.tier_c_draws > 0, "{s1:?}");
    assert!(sn.segments > 1, "{} segments", sn.segments);
    let max = one.iter().zip(&many).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
    assert_eq!(max, 0, "max channel delta across {} segments", sn.segments);
}
