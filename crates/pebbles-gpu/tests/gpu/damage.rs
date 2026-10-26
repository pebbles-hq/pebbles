//! Damage regions (DR-25, M30): a partial frame equals a full redraw, byte for byte.

use super::*;

const SIZE: (u32, u32) = (320, 240);

/// A target texture whose frames persist, rendered through damage-tracking `FrameTargets`.
struct Window {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    targets: FrameTargets,
}

impl Window {
    fn new(device: &wgpu::Device, size: (u32, u32)) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pebbles.gpu.test.damage"),
            size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self { texture, view, targets: FrameTargets::new().with_damage_tracking() }
    }

    fn frame(&mut self, r: &mut Renderer, scene: &Scene) -> pebbles_gpu::FrameStats {
        // A swapchain does not keep its content: start every frame from garbage.
        let target = RenderTarget::new(&self.view, wgpu::TextureFormat::Rgba8Unorm, &mut self.targets);
        r.render(target, scene, &FrameParams::new(Color::WHITE)).expect("render")
    }
}

/// The test UI: a header, eight cards (each a retained fragment) and a caret.
struct Ui {
    header: Scene,
    cards: Vec<Scene>,
    font: peniko::FontData,
}

impl Ui {
    fn new() -> Self {
        let font = peniko::FontData::new(
            peniko::Blob::from(pebbles_render::fonts::builtin_fonts()[0].1.to_vec()),
            0,
        );
        let mut header = Scene::new();
        Painter::new(&mut header).fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(30, 41, 59),
            None,
            &Rect::new(0.0, 0.0, 320.0, 32.0),
        );
        let mut ui = Self { header, cards: Vec::new(), font };
        ui.cards = (0..8).map(|i| ui.card(i, 0)).collect();
        ui
    }

    fn card(&self, i: usize, version: u32) -> Scene {
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        let (x, y) = (8.0 + (i % 4) as f64 * 78.0, 40.0 + (i / 4) as f64 * 96.0);
        let card = kurbo::RoundedRect::new(x, y, x + 70.0, y + 88.0, 6.0);
        p.draw_blurred_rounded_rect(Affine::IDENTITY, card.rect(), Color::BLACK.with_alpha(0.3), 6.0, 3.0);
        let tint = (version * 53 % 200) as u8;
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(240, 240 - tint / 4, 255 - tint),
            None,
            &card,
        );
        p.stroke(&Stroke::new(1.0), Affine::IDENTITY, Color::from_rgb8(tint, 80, 160), None, &card);
        let clip = Rect::new(x + 4.0, y + 4.0, x + 66.0, y + 30.0);
        p.push_layer(Fill::NonZero, peniko::Mix::Normal, 0.8, Affine::IDENTITY, &clip);
        let glyphs = (0..6u32).map(|k| Glyph {
            id: 36 + (k + version) % 30,
            x: (x + 6.0) as f32 + k as f32 * 9.0,
            y: (y + 22.0) as f32,
        });
        p.draw_glyphs(
            &self.font,
            13.0,
            &[],
            &Brush::Solid(Color::BLACK),
            Affine::IDENTITY,
            None,
            Fill::NonZero,
            glyphs,
        );
        p.pop_layer();
        let bar = 10.0 + f64::from(version % 50);
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgb8(37, 99, 235),
            None,
            &Rect::new(x + 6.0, y + 70.0, x + 6.0 + bar, y + 78.0),
        );
        s
    }

    fn scene(&self, caret_on: bool, caret_x: f64) -> Scene {
        let mut s = Scene::new();
        s.append(&self.header, None);
        for card in &self.cards {
            s.append(card, None);
        }
        if caret_on {
            Painter::new(&mut s).fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::BLACK,
                None,
                &Rect::new(caret_x, 200.0, caret_x + 2.0, 220.0),
            );
        }
        s
    }
}

/// DR-21 + DR-25: 200 random edits; after each, the persistent window equals a from-scratch
/// render of the same scene.
#[test]
#[ignore = "gpu"]
fn partial_redraw_is_byte_identical() {
    let Some((device, queue, mut r)) = setup("partial_redraw_is_byte_identical") else { return };
    let mut ui = Ui::new();
    let mut win = Window::new(&device, SIZE);
    let mut rng = 0x2545_F491_4F6C_DD1Du64;
    let mut next = || {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17;
        rng
    };
    let (mut partial, mut caret_x) = (0, 40.0);
    for edit in 0..200u32 {
        match next() % 4 {
            0 | 1 => {
                let i = (next() % 8) as usize;
                ui.cards[i] = ui.card(i, edit + 1);
            }
            2 => caret_x = 20.0 + (next() % 260) as f64,
            _ => {} // nothing changed but the caret blink below
        }
        let scene = ui.scene(edit % 2 == 0, caret_x);
        let st = win.frame(&mut r, &scene);
        if st.full_redraws == 0 {
            partial += 1;
        }
        queue.submit([]);
        let got = readback(&device, &queue, &win.texture);
        let want = render_scene(&device, &queue, &mut r, &scene, SIZE, Color::WHITE);
        if got != want {
            let first = got.iter().zip(&want).position(|(a, b)| a != b).unwrap_or(0) / 4;
            panic!(
                "edit {edit}: partial frame differs from a full redraw at pixel ({}, {}) — damage_px {}\n{st}",
                first as u32 % SIZE.0,
                first as u32 / SIZE.0,
                st.damage_px
            );
        }
    }
    assert!(partial > 100, "most edits should redraw partially, got {partial} of 200");
}

/// A blinking caret over retained content damages one small rect.
#[test]
#[ignore = "gpu"]
fn caret_blink_damages_one_rect() {
    let Some((device, queue, mut r)) = setup("caret_blink_damages_one_rect") else { return };
    let ui = Ui::new();
    let mut win = Window::new(&device, SIZE);
    win.frame(&mut r, &ui.scene(true, 100.0));
    let off = win.frame(&mut r, &ui.scene(false, 100.0));
    assert_eq!(off.full_redraws, 0, "{off}");
    // The 2×20 caret with its 1 px AA margin, the 2 px fringe and the round-out.
    assert!(off.damage_px > 0 && off.damage_px <= (2 + 2 + 4 + 1) * (20 + 2 + 4 + 1), "{off}");
    queue.submit([]);
}

/// A size change redraws everything.
#[test]
#[ignore = "gpu"]
fn resize_forces_full_redraw() {
    let Some((device, queue, mut r)) = setup("resize_forces_full_redraw") else { return };
    let ui = Ui::new();
    let scene = ui.scene(true, 50.0);
    let mut small = Window::new(&device, SIZE);
    small.frame(&mut r, &scene);
    assert_eq!(small.frame(&mut r, &scene).full_redraws, 0, "same size, same scene: nothing to redraw");
    // The same FrameTargets on a larger texture (a resized window's next swapchain image).
    let mut big = Window::new(&device, (400, 300));
    big.targets = std::mem::take(&mut small.targets);
    assert_eq!(big.frame(&mut r, &scene).full_redraws, 1);
    queue.submit([]);
}

/// S57: the performance overlay paints only its top-right panel of a window frame (also on a
/// damage-tracked partial frame), and the offscreen capture of the same scene equals the
/// overlay-off frame byte for byte (the capture API never draws the overlay).
#[test]
#[ignore = "gpu"]
fn perf_overlay_stays_in_its_panel_and_out_of_captures() {
    let Some((device, queue, mut r)) = setup("perf_overlay_stays_in_its_panel_and_out_of_captures") else {
        return;
    };
    let Some(mut capture) = offscreen("perf_overlay_stays_in_its_panel_and_out_of_captures") else { return };
    let ui = Ui::new();
    let scene = ui.scene(true, 100.0);
    let frame = |win: &mut Window, r: &mut Renderer, on: bool| {
        let target = RenderTarget::new(&win.view, wgpu::TextureFormat::Rgba8Unorm, &mut win.targets);
        r.render(target, &scene, &FrameParams::new(Color::WHITE).with_perf_overlay(on)).expect("render");
        readback(&device, &queue, &win.texture)
    };
    let mut off_win = Window::new(&device, SIZE);
    let off = frame(&mut off_win, &mut r, false);
    let mut on_win = Window::new(&device, SIZE);
    frame(&mut on_win, &mut r, true);
    // A second frame: the content is unchanged, so only the overlay panel is damaged.
    let on = frame(&mut on_win, &mut r, true);
    let shot = capture.render_rgba(&scene, SIZE, Color::WHITE, 1.0).expect("capture");
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert_eq!(shot.pixels, off, "the capture equals the overlay-off frame");
    // The panel: 240×96 at 8 px from the top-right corner (scale 1).
    let panel = |x: u32, y: u32| (SIZE.0 - 248..SIZE.0 - 8).contains(&x) && (8..104).contains(&y);
    let (mut inside_diff, mut outside_diff) = (0, 0);
    for y in 0..SIZE.1 {
        for x in 0..SIZE.0 {
            let i = ((y * SIZE.0 + x) * 4) as usize;
            if on[i..i + 4] != off[i..i + 4] {
                if panel(x, y) {
                    inside_diff += 1;
                } else {
                    eprintln!("outside the panel ({x}, {y}): {:?} vs {:?}", &on[i..i + 4], &off[i..i + 4]);
                    outside_diff += 1;
                }
            }
        }
    }
    assert_eq!(outside_diff, 0, "the overlay painted outside its panel");
    assert!(inside_diff > 240 * 96 / 2, "the overlay panel is drawn ({inside_diff} px differ)");
}
