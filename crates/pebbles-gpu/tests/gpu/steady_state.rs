//! Steady-state counters (10 §9.1, M04): FrameStats counts real work.

use super::*;

/// M04 AC2: a 100×100 grid of 8×8 rects in two alternating colors (the §11.1 recipe).
#[test]
#[ignore = "gpu"]
fn stats_count_draws_and_passes() {
    let Some((device, queue, mut r)) = setup("stats_count_draws_and_passes") else { return };
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    for y in 0..100u32 {
        for x in 0..100u32 {
            let c = if (x + y) % 2 == 0 { Color::BLACK } else { Color::from_rgb8(37, 99, 235) };
            let (fx, fy) = (f64::from(x) * 8.0, f64::from(y) * 8.0);
            p.fill(Fill::NonZero, Affine::IDENTITY, c, None, &Rect::new(fx, fy, fx + 8.0, fy + 8.0));
        }
    }
    let _ = render_scene(&device, &queue, &mut r, &s, (800, 800), Color::WHITE);
    let st = r.last_frame_stats();
    assert_eq!(st.render_passes, 1, "{st}");
    assert!(st.draw_calls >= 1, "{st}");
    assert!(st.buffer_bytes_uploaded > 0, "{st}");
    assert_eq!(st.instances, 10_000, "{st}");
}

/// M09 AC1: alternating Rgba8Unorm and Bgra8Unorm targets for 6 frames creates each prewarm
/// pipeline once per format, nothing from frame 3 on, and no validation error.
#[test]
#[ignore = "gpu"]
fn two_formats_create_pipelines_once() {
    let Some((device, queue, mut r)) = setup("two_formats_create_pipelines_once") else { return };
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &Rect::new(4.0, 4.0, 20.0, 20.0));
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::BLACK,
        None,
        &kurbo::RoundedRect::new(24.0, 4.0, 40.0, 20.0, 4.0),
    );
    let formats = [wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureFormat::Bgra8Unorm];
    let textures = formats.map(|f| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pebbles.gpu.test.formats"),
            size: wgpu::Extent3d { width: 64, height: 32, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: f,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    });
    let mut targets = [FrameTargets::new(), FrameTargets::new()];
    let prewarm_set = 5; // solid, sdf, shadow, glyph, image (K3)
    let mut total = 0;
    for frame in 1..=6usize {
        let i = frame % 2;
        let view = textures[i].create_view(&wgpu::TextureViewDescriptor::default());
        let st = r
            .render(
                RenderTarget::new(&view, formats[i], &mut targets[i]),
                &s,
                &FrameParams::new(Color::WHITE),
            )
            .expect("render");
        total += st.pipelines_created;
        if frame >= 3 {
            assert_eq!(st.pipelines_created, 0, "frame {frame}: {st}");
        }
    }
    queue.submit([]);
    assert_eq!(total, 2 * prewarm_set);
}

/// M12 AC1: text + 3 images + 2 sampler modes — frames 3–10 create no bind group and no sampler.
#[test]
#[ignore = "gpu"]
fn bind_groups_not_recreated() {
    let Some((device, queue, mut r)) = setup("bind_groups_not_recreated") else { return };
    let font =
        peniko::FontData::new(peniko::Blob::from(pebbles_render::fonts::builtin_fonts()[0].1.to_vec()), 0);
    let images: Vec<peniko::ImageData> = (0..3u8).map(|i| rgba_image(4, 4, [i * 80, 40, 200, 255])).collect();
    let mut targets = FrameTargets::new();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.test.steady"),
        size: wgpu::Extent3d { width: 128, height: 64, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    for frame in 1..=10u32 {
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        let glyphs = (0..8u32).map(|i| Glyph { id: 36 + i, x: 4.0 + i as f32 * 9.0, y: 50.0 });
        p.draw_glyphs(
            &font,
            14.0,
            &[],
            &Brush::Solid(Color::BLACK),
            Affine::IDENTITY,
            None,
            Fill::NonZero,
            glyphs,
        );
        for (i, img) in images.iter().enumerate() {
            let mut brush = peniko::ImageBrush::new(img.clone());
            // Two sampler modes: Low (nearest) for the first image, the default for the others.
            if i == 0 {
                brush.sampler.quality = peniko::ImageQuality::Low;
            }
            p.draw_image(&brush, Affine::translate((4.0 + i as f64 * 20.0, 4.0)) * Affine::scale(4.0));
        }
        let st = r
            .render(
                RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, &mut targets),
                &s,
                &FrameParams::new(Color::WHITE),
            )
            .expect("render");
        if frame >= 3 {
            assert_eq!((st.bind_groups_created, st.samplers_created), (0, 0), "frame {frame}: {st}");
        }
    }
    queue.submit([]);
}

// ── M19: deterministic steady-state count gates and draw-call ceilings (10 §11.4) ──────────

const SIZE: (u32, u32) = (400, 300);
const SCENES: [&str; 7] =
    ["dashboard", "rects_10k", "rrects_2k", "paths_2k", "glyph_runs_1k", "images_64", "shadows_200"];

/// One of the §11.4 gate scenes at 400×300. Images are built once per call and reused across
/// frames by the caller (an image created per frame would be a new upload, by design).
fn gate_scene(name: &str, images: &[peniko::ImageData]) -> Scene {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    let font =
        peniko::FontData::new(peniko::Blob::from(pebbles_render::fonts::builtin_fonts()[0].1.to_vec()), 0);
    let color = |i: u32| Color::from_rgb8((i * 37 % 256) as u8, (i * 91 % 256) as u8, (i * 53 % 256) as u8);
    let glyph_run = |p: &mut Painter<'_>, i: u32, x: f64, y: f64| {
        let glyphs = (0..12u32).map(|k| Glyph {
            id: 36 + (i + k) % 40,
            x: (x + f64::from(k) * 7.0) as f32,
            y: y as f32,
        });
        p.draw_glyphs(
            &font,
            11.0,
            &[],
            &Brush::Solid(Color::BLACK),
            Affine::IDENTITY,
            None,
            Fill::NonZero,
            glyphs,
        );
    };
    match name {
        "rects_10k" => {
            for i in 0..10_000u32 {
                let (x, y) = (f64::from(i % 100) * 4.0, f64::from(i / 100) * 3.0);
                p.fill(Fill::NonZero, Affine::IDENTITY, color(i), None, &Rect::new(x, y, x + 3.0, y + 2.0));
            }
        }
        "rrects_2k" => {
            for i in 0..2_000u32 {
                let (x, y) = (f64::from(i % 50) * 8.0, f64::from(i / 50) * 7.5);
                let rr = kurbo::RoundedRect::new(x, y, x + 7.0, y + 6.0, 2.0);
                p.fill(Fill::NonZero, Affine::IDENTITY, color(i), None, &rr);
            }
        }
        "paths_2k" => {
            for i in 0..2_000u32 {
                let (x, y) = (f64::from(i % 50) * 8.0, f64::from(i / 50) * 7.5);
                let mut path = kurbo::BezPath::new();
                path.move_to((x, y + 6.0));
                path.quad_to((x + 3.5, y - 2.0), (x + 7.0, y + 6.0));
                path.close_path();
                p.fill(Fill::NonZero, Affine::IDENTITY, color(i), None, &path);
            }
        }
        "glyph_runs_1k" => {
            for i in 0..1_000u32 {
                glyph_run(&mut p, i, f64::from(i % 4) * 100.0, 10.0 + f64::from(i / 4) * 1.15);
            }
        }
        "images_64" => {
            for (i, img) in images.iter().enumerate() {
                let (x, y) = ((i % 8) as f64 * 50.0, (i / 8) as f64 * 37.0);
                p.draw_image(
                    &peniko::ImageBrush::new(img.clone()),
                    Affine::translate((x, y)) * Affine::scale(4.0),
                );
            }
        }
        "shadows_200" => {
            for i in 0..200u32 {
                let (x, y) = (f64::from(i % 20) * 20.0, f64::from(i / 20) * 30.0);
                p.draw_blurred_rounded_rect(
                    Affine::IDENTITY,
                    Rect::new(x + 2.0, y + 2.0, x + 16.0, y + 24.0),
                    Color::BLACK,
                    3.0,
                    2.0,
                );
            }
        }
        // A small app screen: cards with shadows, a header, text and a stroked chart line.
        _ => {
            p.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgb8(30, 41, 59),
                None,
                &Rect::new(0.0, 0.0, 400.0, 40.0),
            );
            for c in 0..6u32 {
                let (x, y) = (10.0 + f64::from(c % 3) * 130.0, 50.0 + f64::from(c / 3) * 120.0);
                let card = kurbo::RoundedRect::new(x, y, x + 120.0, y + 110.0, 8.0);
                p.draw_blurred_rounded_rect(
                    Affine::IDENTITY,
                    card.rect(),
                    Color::BLACK.with_alpha(0.2),
                    8.0,
                    4.0,
                );
                p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &card);
                glyph_run(&mut p, c, x + 8.0, y + 20.0);
                let mut line = kurbo::BezPath::new();
                line.move_to((x + 8.0, y + 90.0));
                for k in 1..10u32 {
                    line.line_to((x + 8.0 + f64::from(k) * 11.0, y + 90.0 - f64::from((k * 7 + c) % 40)));
                }
                p.stroke(&Stroke::new(1.5), Affine::IDENTITY, color(c), None, &line);
            }
            if let Some(img) = images.first() {
                p.draw_image(
                    &peniko::ImageBrush::new(img.clone()),
                    Affine::translate((370.0, 8.0)) * Affine::scale(6.0),
                );
            }
        }
    }
    s
}

fn gate_images() -> Vec<peniko::ImageData> {
    (0..64u32).map(|i| rgba_image(4, 4, [(i * 4) as u8, 120, 200, 255])).collect()
}

/// Render `name` for 10 frames into one target through one renderer (render-only path) and
/// return each frame's stats.
fn ten_frames(r: &mut Renderer, device: &wgpu::Device, name: &str) -> Vec<pebbles_gpu::FrameStats> {
    let images = gate_images();
    let scene = gate_scene(name, &images);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.test.gate"),
        size: wgpu::Extent3d { width: SIZE.0, height: SIZE.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut targets = FrameTargets::new();
    (1..=10)
        .map(|_| {
            r.render(
                RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, &mut targets),
                &scene,
                &FrameParams::new(Color::WHITE),
            )
            .expect("render")
        })
        .collect()
}

/// B5: from frame 3 on, a steady scene creates no GPU object and uploads no texture data.
#[test]
#[ignore = "gpu"]
fn steady_state_creates_nothing() {
    for name in SCENES {
        // One renderer per scene: the gate is about each scene's own steady state.
        let Some((device, queue, mut r)) = setup("steady_state_creates_nothing") else { return };
        for (i, st) in ten_frames(&mut r, &device, name).iter().enumerate().skip(2) {
            let frame = i + 1;
            for (field, value) in [
                ("buffers_created", st.buffers_created),
                ("textures_created", st.textures_created),
                ("texture_views_created", st.texture_views_created),
                ("bind_groups_created", st.bind_groups_created),
                ("samplers_created", st.samplers_created),
                ("pipelines_created", st.pipelines_created),
                ("glyphs_rasterized", st.glyphs_rasterized),
                ("images_uploaded", st.images_uploaded),
            ] {
                assert_eq!(value, 0, "{name} frame {frame}: {field} = {value}\n{st}");
            }
            assert_eq!(st.texture_bytes_uploaded, 0, "{name} frame {frame}: texture_bytes_uploaded\n{st}");
        }
        queue.submit([]);
    }
}

/// Glyphs upload on the first frame only.
#[test]
#[ignore = "gpu"]
fn glyph_upload_only_on_first_frame() {
    let Some((device, queue, mut r)) = setup("glyph_upload_only_on_first_frame") else { return };
    let frames = ten_frames(&mut r, &device, "glyph_runs_1k");
    assert!(frames[0].texture_bytes_uploaded > 0, "frame 1 uploads glyphs\n{}", frames[0]);
    for (i, st) in frames.iter().enumerate().skip(1) {
        assert_eq!(st.texture_bytes_uploaded, 0, "frame {}\n{st}", i + 1);
    }
    queue.submit([]);
}

// Calibrated 2026-10-24 at 60e7661. Ratchet: only lower these; raising needs a DR (10 §1.7).
/// `(scene, draw_calls, pipeline_switches, render_passes)` ceilings (B7).
const CEILINGS: &[(&str, u32, u32, u32)] = &[
    ("dashboard", 20, 20, 1),
    ("rects_10k", 1, 1, 1),
    ("rrects_2k", 1, 1, 1),
    ("paths_2k", 1, 1, 1),
    ("glyph_runs_1k", 1, 1, 1),
    ("images_64", 64, 1, 1),
    ("shadows_200", 1, 1, 1),
];

/// B7: the steady frame of each scene stays under its draw-call, pipeline-switch and pass
/// ceilings. `PEBBLES_PRINT_CEILINGS=1` prints the measured values instead (calibration).
#[test]
#[ignore = "gpu"]
fn draw_call_ceilings() {
    let print = std::env::var("PEBBLES_PRINT_CEILINGS").as_deref() == Ok("1");
    for &(name, draws, switches, passes) in CEILINGS {
        let Some((device, queue, mut r)) = setup("draw_call_ceilings") else { return };
        let st = ten_frames(&mut r, &device, name).pop().expect("ten frames");
        queue.submit([]);
        if print {
            println!("    (\"{name}\", {}, {}, {}),", st.draw_calls, st.pipeline_switches, st.render_passes);
            continue;
        }
        assert!(st.draw_calls <= draws, "{name}: draw_calls {} > {draws}\n{st}", st.draw_calls);
        assert!(
            st.pipeline_switches <= switches,
            "{name}: pipeline_switches {} > {switches}\n{st}",
            st.pipeline_switches
        );
        assert!(st.render_passes <= passes, "{name}: render_passes {} > {passes}\n{st}", st.render_passes);
    }
}
