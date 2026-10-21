//! V08: every cache stays stable over many frames (07 §2 defects: vanishing text, per-frame
//! allocations, unbounded image cache, multi-target thrash, size races).

use pebbles_gpu::testing::bench_scenes;

use super::*;

fn font() -> peniko::FontData {
    peniko::FontData::new(peniko::Blob::from(pebbles_render::fonts::builtin_fonts()[0].1.to_vec()), 0)
}

fn render_to(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    r: &mut Renderer,
    t: &mut FrameTargets,
    s: &Scene,
    size: (u32, u32),
    scale: f64,
) -> Vec<u8> {
    let texture = target_texture(device, size.0, size.1);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let target = RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, t);
    r.render(target, s, &FrameParams::new(Color::WHITE).with_root_transform(Affine::scale(scale)))
        .expect("render");
    readback(device, queue, &texture)
}

fn dashboard() -> Scene {
    let mut s = Scene::new();
    bench_scenes::dashboard(&mut Painter::new(&mut s));
    s
}

#[test]
#[ignore = "gpu"]
fn glyphs_survive_500_frames() {
    let Some((device, queue, mut r)) = setup("glyphs_survive_500_frames") else { return };
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    let f = font();
    for i in 0..40u32 {
        let size = if i % 2 == 0 { 12.0 } else { 24.0 };
        let glyphs: Vec<Glyph> =
            (0..12).map(|k| Glyph { id: 36 + (i + k) % 30, x: k as f32 * size * 0.6, y: 0.0 }).collect();
        p.draw_glyphs(
            &f,
            size,
            &[],
            &Brush::Solid(Color::BLACK),
            Affine::translate((4.0 + f64::from(i % 2) * 200.0, 14.0 + f64::from(i / 2) * 14.0)),
            None,
            Fill::NonZero,
            glyphs.into_iter(),
        );
    }
    let mut t = FrameTargets::new();
    let first = render_to(&device, &queue, &mut r, &mut t, &s, (400, 300), 1.0);
    let mut last = Vec::new();
    for _ in 1..500 {
        last = render_to(&device, &queue, &mut r, &mut t, &s, (400, 300), 1.0);
    }
    assert!(first == last, "frame 500 differs from frame 1");
}

#[test]
#[ignore = "gpu"]
fn dashboard_frame_300_equals_frame_1() {
    let Some((device, queue, mut r)) = setup("dashboard_frame_300_equals_frame_1") else { return };
    let s = dashboard();
    let mut t = FrameTargets::new();
    let first = render_to(&device, &queue, &mut r, &mut t, &s, (600, 400), 0.5);
    let mut last = Vec::new();
    for _ in 1..300 {
        last = render_to(&device, &queue, &mut r, &mut t, &s, (600, 400), 0.5);
    }
    assert!(first == last);
}

#[test]
#[ignore = "gpu"]
fn steady_state_creates_no_gpu_objects() {
    let Some((device, queue, mut r)) = setup("steady_state_creates_no_gpu_objects") else { return };
    let s = dashboard();
    let mut t = FrameTargets::new();
    for _ in 0..3 {
        render_to(&device, &queue, &mut r, &mut t, &s, (600, 400), 0.5);
    }
    for i in 0..10 {
        render_to(&device, &queue, &mut r, &mut t, &s, (600, 400), 0.5);
        let st = r.last_frame_stats();
        assert_eq!(
            (st.buffers_created, st.textures_created, st.bind_groups_created, st.texture_bytes_uploaded),
            (0, 0, 0, 0),
            "steady frame {i}: {st:?}"
        );
    }
}

#[test]
#[ignore = "gpu"]
fn image_churn_stays_within_budget() {
    const BUDGET: u64 = 8 << 20;
    let Some((device, queue, mut r)) =
        setup_with_budgets("image_churn_stays_within_budget", |b| b.image_cache_bytes = BUDGET)
    else {
        return;
    };
    let images: Vec<_> = (0..100u32)
        .map(|i| {
            let px: Vec<u8> = (0..256 * 256u32)
                .flat_map(|k| [(k + i).to_le_bytes()[0], (k >> 8).to_le_bytes()[0], i.to_le_bytes()[0], 255])
                .collect();
            peniko::ImageBrush::new(peniko::ImageData {
                data: peniko::Blob::from(px),
                ..rgba_image(256, 256, [0; 4])
            })
        })
        .collect();
    let frame = |f: usize| {
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        for k in 0..6 {
            p.draw_image(
                &images[(f * 6 + k) % 100],
                Affine::translate(((k as f64) * 66.0, 0.0)) * Affine::scale(0.25),
            );
        }
        s
    };
    let mut t = FrameTargets::new();
    let mut last = Vec::new();
    for f in 0..300 {
        last = render_to(&device, &queue, &mut r, &mut t, &frame(f), (400, 64), 1.0);
        let bytes = r.last_frame_stats().image_cache_bytes;
        // In-use images may overshoot the soft budget by this frame's 6 (10 §6).
        assert!(bytes <= BUDGET + 6 * 256 * 256 * 4, "frame {f}: {bytes} B");
    }
    let Some((d2, q2, mut fresh)) = setup("image_churn fresh") else { return };
    let mut t2 = FrameTargets::new();
    assert!(
        render_to(&d2, &q2, &mut fresh, &mut t2, &frame(299), (400, 64), 1.0) == last,
        "frame 300 == a fresh render"
    );
}

#[test]
#[ignore = "gpu"]
fn two_target_sizes_do_not_thrash() {
    let Some((device, queue, mut r)) = setup("two_target_sizes_do_not_thrash") else { return };
    let s = dashboard();
    let (mut a, mut b) = (FrameTargets::new(), FrameTargets::new());
    for i in 0..100 {
        if i % 2 == 0 {
            render_to(&device, &queue, &mut r, &mut a, &s, (300, 200), 0.25);
        } else {
            render_to(&device, &queue, &mut r, &mut b, &s, (500, 400), 0.4);
        }
    }
    #[cfg(debug_assertions)]
    assert_eq!(r.census().frame_target_entries, 2);
}

#[test]
#[ignore = "gpu"]
fn resize_sequence_matches_fresh_renderer() {
    let Some((device, queue, mut r)) = setup("resize_sequence_matches_fresh_renderer") else { return };
    let s = dashboard();
    let mut t = FrameTargets::new();
    for i in 0..10u32 {
        let size = (800 + i, 600);
        let got = render_to(&device, &queue, &mut r, &mut t, &s, size, 0.6);
        let Some((d2, q2, mut fresh)) = setup("resize fresh") else { return };
        let mut t2 = FrameTargets::new();
        assert!(got == render_to(&d2, &q2, &mut fresh, &mut t2, &s, size, 0.6), "size {size:?}");
    }
}
