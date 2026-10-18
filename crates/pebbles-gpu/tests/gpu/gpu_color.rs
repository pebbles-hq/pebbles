//! R08: the color contract proven with pixel values (02 §3) — gamma-space premultiplied
//! blending into Unorm targets, exact clears, raw (premultiplied) readback.

use super::*;

const W: u32 = 64;

/// Render `scene` on `base` into a `format` texture created with `view_formats`, through a view
/// of `view_format`; read the raw bytes back.
fn render_into(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    scene: &Scene,
    base: Color,
    format: wgpu::TextureFormat,
    view_format: wgpu::TextureFormat,
) -> Vec<u8> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.test.color"),
        size: wgpu::Extent3d { width: W, height: W, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: if view_format == format { &[] } else { std::slice::from_ref(&view_format) },
    });
    let view =
        texture.create_view(&wgpu::TextureViewDescriptor { format: Some(view_format), ..Default::default() });
    let mut targets = FrameTargets::new();
    let target = RenderTarget::new(&view, view_format, &mut targets);
    renderer.render(target, scene, &FrameParams::new(base)).expect("render");
    queue.submit([]);
    readback(device, queue, &texture)
}

fn rgba(device: &wgpu::Device, queue: &wgpu::Queue, r: &mut Renderer, scene: &Scene, base: Color) -> Vec<u8> {
    let f = wgpu::TextureFormat::Rgba8Unorm;
    render_into(device, queue, r, scene, base, f, f)
}

fn fill(p: &mut Painter<'_>, rect: Rect, c: Color) {
    p.fill(Fill::NonZero, Affine::IDENTITY, c, None, &rect);
}

const BOX: Rect = Rect::new(8.0, 8.0, 56.0, 56.0);

fn near(got: [u8; 4], want: [i32; 3], tol: i32) -> bool {
    (0..3).all(|i| (i32::from(got[i]) - want[i]).abs() <= tol)
}

fn half_black_scene() -> Scene {
    let mut s = Scene::new();
    fill(&mut Painter::new(&mut s), BOX, Color::from_rgba8(0, 0, 0, 128));
    s
}

fn overlap_scene() -> Scene {
    let mut s = Scene::new();
    let p = &mut Painter::new(&mut s);
    fill(p, Rect::new(8.0, 8.0, 40.0, 56.0), Color::from_rgba8(255, 0, 0, 128));
    fill(p, Rect::new(24.0, 8.0, 56.0, 56.0), Color::from_rgba8(0, 0, 255, 128));
    s
}

#[test]
#[ignore = "gpu"]
fn clear_color_exact() {
    let Some((device, queue, mut r)) = setup("clear_color_exact") else { return };
    let px = rgba(&device, &queue, &mut r, &Scene::new(), Color::from_rgba8(0x80, 0x80, 0x80, 0xFF));
    assert!(px.as_chunks::<4>().0.iter().all(|p| *p == [0x80, 0x80, 0x80, 0xFF]));
}

#[test]
#[ignore = "gpu"]
fn opaque_fill_roundtrip() {
    let Some((device, queue, mut r)) = setup("opaque_fill_roundtrip") else { return };
    let mut s = Scene::new();
    fill(&mut Painter::new(&mut s), BOX, Color::from_rgba8(0x33, 0x66, 0xCC, 0xFF));
    let px = rgba(&device, &queue, &mut r, &s, Color::WHITE);
    assert_eq!(pixel(&px, W, 32, 32), [0x33, 0x66, 0xCC, 0xFF]);
}

#[test]
#[ignore = "gpu"]
fn half_black_over_white() {
    let Some((device, queue, mut r)) = setup("half_black_over_white") else { return };
    let px = rgba(&device, &queue, &mut r, &half_black_scene(), Color::WHITE);
    let c = pixel(&px, W, 32, 32);
    assert!((0..3).all(|i| (126..=129).contains(&c[i])), "{c:?} (≈188 means linear blending)");
}

#[test]
#[ignore = "gpu"]
fn translucent_overlap() {
    let Some((device, queue, mut r)) = setup("translucent_overlap") else { return };
    let px = rgba(&device, &queue, &mut r, &overlap_scene(), Color::WHITE);
    let (mid, left) = (pixel(&px, W, 32, 32), pixel(&px, W, 16, 32));
    assert!(near(mid, [128, 64, 191], 2), "overlap {mid:?}");
    assert!(near(left, [255, 127, 127], 2), "red only {left:?}");
}

#[test]
#[ignore = "gpu"]
fn half_covered_edge_column() {
    let Some((device, queue, mut r)) = setup("half_covered_edge_column") else { return };
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    // Exact coverage, not the pixel-grid policy (R18 snaps this edge under `Auto`).
    p.set_pixel_snap(pebbles_render::paint::display_list::PixelSnap::Off);
    fill(&mut p, Rect::new(10.5, 8.0, 30.0, 56.0), Color::BLACK);
    let px = rgba(&device, &queue, &mut r, &s, Color::WHITE);
    for y in 9..=54 {
        let edge = pixel(&px, W, 10, y);
        assert!((0..3).all(|i| (125..=130).contains(&edge[i])), "edge ({y}) {edge:?}");
        assert_eq!(pixel(&px, W, 11, y), [0, 0, 0, 255], "inside ({y})");
    }
}

#[test]
#[ignore = "gpu"]
fn premultiplied_clear_is_premultiplied() {
    let Some((device, queue, mut r)) = setup("premultiplied_clear_is_premultiplied") else { return };
    let px = rgba(&device, &queue, &mut r, &Scene::new(), Color::from_rgba8(0x80, 0x80, 0x80, 0x80));
    let c = pixel(&px, W, 5, 5);
    assert!(near(c, [64, 64, 64], 1) && (i32::from(c[3]) - 128).abs() <= 1, "{c:?}");
}

#[test]
#[ignore = "gpu"]
fn bgra_target_matches_rgba() {
    let Some((device, queue, mut r)) = setup("bgra_target_matches_rgba") else { return };
    let scene = overlap_scene();
    let want = rgba(&device, &queue, &mut r, &scene, Color::WHITE);
    let f = wgpu::TextureFormat::Bgra8Unorm;
    let mut got = render_into(&device, &queue, &mut r, &scene, Color::WHITE, f, f);
    for p in got.as_chunks_mut::<4>().0 {
        p.swap(0, 2);
    }
    assert!(got == want, "BGRA bytes differ from RGBA after the swap");
}

#[test]
#[ignore = "gpu"]
fn srgb_storage_with_unorm_view() {
    let Some((device, queue, mut r)) = setup("srgb_storage_with_unorm_view") else { return };
    let want = rgba(&device, &queue, &mut r, &half_black_scene(), Color::WHITE);
    let (s, u) = (wgpu::TextureFormat::Rgba8UnormSrgb, wgpu::TextureFormat::Rgba8Unorm);
    let got = render_into(&device, &queue, &mut r, &half_black_scene(), Color::WHITE, s, u);
    assert!(got == want, "sRGB storage through a Unorm view must store the same bytes");
}

/// R13 AC1: on a GL-style sRGB-only target (`Rgba8UnormSrgb`, no view formats) with
/// `decode_srgb_output`, opaque colors read back as the encoded values; translucent pixels blend
/// in linear light (the documented deviation, 02 §3.8: half-black on white ≈ 188, not 128).
#[test]
#[ignore = "gpu"]
fn gl_srgb_fallback_opaque_matches() {
    let Some((device, queue, mut r)) = setup("gl_srgb_fallback_opaque_matches") else { return };
    let render = |r: &mut Renderer, scene: &Scene| {
        let f = wgpu::TextureFormat::Rgba8UnormSrgb;
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pebbles.gpu.test.srgb_fallback"),
            size: wgpu::Extent3d { width: W, height: W, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: f,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut targets = FrameTargets::new();
        let target = RenderTarget::new(&view, f, &mut targets).with_decode_srgb_output(true);
        r.render(target, scene, &FrameParams::new(Color::WHITE)).expect("render");
        queue.submit([]);
        readback(&device, &queue, &texture)
    };
    let mut opaque = Scene::new();
    fill(&mut Painter::new(&mut opaque), BOX, Color::from_rgba8(0x33, 0x66, 0xCC, 0xFF));
    let px = render(&mut r, &opaque);
    let c = pixel(&px, W, 30, 30);
    assert!(near(c, [0x33, 0x66, 0xCC], 1), "opaque fill {c:?}");
    assert!(near(pixel(&px, W, 2, 2), [255, 255, 255], 1), "white base");
    let px = render(&mut r, &half_black_scene());
    let c = pixel(&px, W, 30, 30);
    assert!((0..3).all(|i| (180..=195).contains(&c[i])), "half-black blends in linear light: {c:?}");
}
