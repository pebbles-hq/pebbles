//! Phase-1 renderer-backend de-risk harness.
//!
//! Renders a Pebbles-representative dashboard through BOTH candidate backends —
//! the default-candidate hybrid raster path (`vello_hybrid`) and the opt-in
//! compute path (`vello`) — headless, and answers the two Phase-1 questions:
//!
//!   1. FEATURE COVERAGE — every paint primitive the widget layer emits, probed
//!      on the hybrid backend under `catch_unwind`. The known gap is the
//!      luminance-mask layer (ShaderMask) + certain non-isolated blends; this
//!      stage proves exactly which primitives render and which panic, with NO
//!      GPU required (the panics fire during CPU scene-building).
//!
//!   2. RENDER + TIMING — if a GPU adapter is present, rasterize the same scene
//!      through both backends to PNG and time N frames each (both share ONE wgpu
//!      device — they resolve to the same wgpu). If no adapter is available in
//!      this environment, the stage is skipped and the on-hardware watt-capture
//!      commands are printed for the operator to run.
//!
//! This is a throwaway spike in a detached workspace; the real gate is the
//! operator's on-hardware visual + power check.

use std::panic::{self, AssertUnwindSafe};
use std::time::Instant;

use kurbo::{Affine, BezPath, Point, Rect, RoundedRect, Shape, Stroke};
use peniko::{BlendMode, Brush, Color, Compose, Fill, Gradient, Mix};

use vello_common::mask::Mask;

use vello::AaConfig;
use vello::{RenderParams, Renderer as VRenderer, RendererOptions, Scene as VScene};
use vello_hybrid::{
    RenderSize, RenderTargetConfig, Renderer as HRenderer, Scene as HScene, TextureBindings,
};

const W: u16 = 1200;
const H: u16 = 800;
const FRAMES: u32 = 120;

fn main() {
    println!("======================================================================");
    println!(" Pebbles renderer-backend de-risk harness  (Phase 1)");
    println!(" default-candidate: vello_hybrid (hybrid CPU+GPU raster, battery)");
    println!(" opt-in:            vello         (GPU compute raster, heavy vector)");
    println!(" surface: {W}x{H}, both backends on the SAME wgpu 29 device");
    println!("======================================================================\n");

    coverage_probe();

    match pollster::block_on(render_and_time()) {
        Ok(()) => {}
        Err(e) => {
            println!("\n[render+timing] SKIPPED — no usable GPU in this environment:");
            println!("    {e}");
            println!("  The coverage matrix above needs no GPU and is the decisive");
            println!("  Phase-1 result. Run this binary on real hardware for watts.");
        }
    }

    print_watt_commands();
}

// ---------------------------------------------------------------------------
// Stage 1 — feature coverage on the hybrid backend (no GPU needed)
// ---------------------------------------------------------------------------

/// Every paint primitive Pebbles' `Painter` seam emits, exercised on a fresh
/// `vello_hybrid::Scene`. A primitive that panics during scene-building is a gap
/// we must close (workaround, upstream, or route-to-full-Vello) before flipping
/// the default. `catch_unwind` keeps one panic from killing the whole matrix.
fn coverage_probe() {
    println!("── Stage 1: hybrid feature coverage (CPU scene-build, no GPU) ──\n");

    // Silence the default panic hook so expected panics don't dump backtraces
    // mid-table; we report them ourselves.
    let prev = panic::take_hook();
    panic::set_hook(Box::new(|_| {}));

    let cases: Vec<(&str, fn())> = vec![
        ("solid fill_rect", || {
            let mut s = HScene::new(W, H);
            s.set_paint(Color::from_rgb8(200, 40, 50));
            s.fill_rect(&Rect::new(0.0, 0.0, 120.0, 80.0));
        }),
        ("rounded-rect fill_path (card body)", || {
            let mut s = HScene::new(W, H);
            s.set_paint(Color::from_rgb8(30, 33, 46));
            s.fill_path(&card_path(Rect::new(0.0, 0.0, 200.0, 120.0)));
        }),
        ("stroke_path (card border)", || {
            let mut s = HScene::new(W, H);
            s.set_stroke(Stroke::new(1.5));
            s.set_paint(Color::from_rgba8(255, 255, 255, 40));
            s.stroke_path(&card_path(Rect::new(0.0, 0.0, 200.0, 120.0)));
        }),
        ("linear gradient (hero band)", || {
            let mut s = HScene::new(W, H);
            s.set_paint(hero_gradient());
            s.fill_rect(&Rect::new(0.0, 0.0, W as f64, 220.0));
        }),
        ("radial gradient (glow)", || {
            let mut s = HScene::new(W, H);
            s.set_paint(glow_gradient());
            s.fill_rect(&Rect::new(0.0, 0.0, 300.0, 300.0));
        }),
        ("blurred rounded rect (drop shadow)", || {
            let mut s = HScene::new(W, H);
            s.set_paint(Color::from_rgba8(0, 0, 0, 110));
            s.fill_blurred_rounded_rect(&Rect::new(20.0, 20.0, 220.0, 140.0), 16.0, 18.0, false);
        }),
        ("clip layer (rounded clip)", || {
            let mut s = HScene::new(W, H);
            s.push_clip_layer(&card_path(Rect::new(0.0, 0.0, 200.0, 120.0)));
            s.set_paint(Color::from_rgb8(80, 120, 255));
            s.fill_rect(&Rect::new(0.0, 0.0, 200.0, 120.0));
            s.pop_layer();
        }),
        ("opacity layer (translucent overlay)", || {
            let mut s = HScene::new(W, H);
            s.push_opacity_layer(0.7);
            s.set_paint(Color::from_rgb8(255, 255, 255));
            s.fill_rect(&Rect::new(0.0, 0.0, 200.0, 120.0));
            s.pop_layer();
        }),
        ("blend layer: Multiply (isolated)", || {
            let mut s = HScene::new(W, H);
            s.push_blend_layer(BlendMode::from(Mix::Multiply));
            s.set_paint(Color::from_rgb8(255, 120, 60));
            s.fill_rect(&Rect::new(0.0, 0.0, 200.0, 120.0));
            s.pop_layer();
        }),
        ("blend layer: Screen (isolated)", || {
            let mut s = HScene::new(W, H);
            s.push_blend_layer(BlendMode::from(Mix::Screen));
            s.set_paint(Color::from_rgb8(60, 200, 255));
            s.fill_rect(&Rect::new(0.0, 0.0, 200.0, 120.0));
            s.pop_layer();
        }),
        ("LUMINANCE MASK layer (ShaderMask)", || {
            // Pebbles' `push_luminance_mask_layer` maps here. Documented gap:
            // vello_hybrid 0.2 panics `unimplemented!("mask layers ...")`.
            let mut s = HScene::new(W, H);
            let mask = Mask::from_parts(vec![128u8; 64 * 64], 64, 64);
            s.push_mask_layer(mask);
            s.set_paint(Color::from_rgb8(255, 255, 255));
            s.fill_rect(&Rect::new(0.0, 0.0, 64.0, 64.0));
            s.pop_layer();
        }),
        ("ShaderMask via DestIn emulation (Phase-3 fix)", || {
            // What Pebbles' RenderShaderMask now emits instead of the mask-layer API:
            // isolate the child, then DestIn a gradient whose per-stop alpha = luminance.
            // Uses only the isolated-layer + DestIn verbs vello_hybrid DOES support.
            let mut s = HScene::new(W, H);
            let bounds = Rect::new(0.0, 0.0, 200.0, 120.0);
            let clip = bounds.to_path(0.1);
            s.push_layer(Some(&clip), None, None, None, None); // isolate child
            s.set_paint(Color::from_rgb8(80, 120, 255));
            s.fill_rect(&bounds);
            s.push_layer(
                Some(&clip),
                Some(BlendMode::new(Mix::Normal, Compose::DestIn)),
                None,
                None,
                None,
            );
            s.set_paint(luminance_fade()); // white α=1 → white α=0
            s.fill_rect(&bounds);
            s.pop_layer();
            s.pop_layer();
        }),
    ];

    let mut supported = 0usize;
    let mut gaps: Vec<&str> = Vec::new();
    for (name, case) in &cases {
        let ok = panic::catch_unwind(AssertUnwindSafe(case)).is_ok();
        if ok {
            supported += 1;
            println!("  [ OK   ] {name}");
        } else {
            gaps.push(name);
            println!("  [ PANIC] {name}   <-- gap to close before default flip");
        }
    }

    panic::set_hook(prev);

    println!(
        "\n  coverage: {}/{} primitives render on vello_hybrid 0.2",
        supported,
        cases.len()
    );
    if gaps.is_empty() {
        println!("  no gaps — hybrid covers Pebbles' full paint surface.");
    } else {
        println!("  GAPS ({}):", gaps.len());
        for g in &gaps {
            println!("    - {g}");
        }
        println!("  → Phase-3 decisions: emulate in the Painter (a luminance mask can");
        println!("    be composited from an offscreen alpha pass), upstream a fix, or");
        println!("    route just that surface through full Vello. No silent wrong pixels.");
    }
    println!();
}

// ---------------------------------------------------------------------------
// Stage 2 — headless render + frame timing through both backends
// ---------------------------------------------------------------------------

async fn render_and_time() -> Result<(), String> {
    println!("── Stage 2: headless render + frame timing (needs a GPU adapter) ──\n");

    // ONE wgpu device drives BOTH backends — they resolve to the same wgpu 29,
    // which is the whole reason they can share a device behind one seam.
    let instance = wgpu::Instance::default();
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        })
        .await
        .map_err(|e| format!("no wgpu adapter: {e:?}"))?;
    let info = adapter.get_info();
    println!(
        "  adapter: {} ({:?}, {:?})",
        info.name, info.device_type, info.backend
    );

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("renderer-bench device"),
            required_features: wgpu::Features::empty(),
            ..Default::default()
        })
        .await
        .map_err(|e| format!("request_device failed: {e:?}"))?;

    // --- full Vello ---
    let vello_ms = time_vello(&device, &queue)?;
    println!(
        "  vello (compute)  : {:.3} ms/frame  over {FRAMES} frames  -> vello.png",
        vello_ms
    );

    // --- Vello Hybrid ---
    let hybrid_ms = time_hybrid(&device, &queue)?;
    println!(
        "  vello_hybrid     : {:.3} ms/frame  over {FRAMES} frames  -> hybrid.png",
        hybrid_ms
    );

    // Phase-4 flush: record a scene through the REAL framework `Painter`, flush the
    // op-list into a vello_hybrid::Scene, and render it.
    flush_demo(&device, &queue)?;
    println!("  pebbles flush    : Painter -> op-list -> vello_hybrid::Scene -> render  -> flush.png");

    println!(
        "\n  NOTE: frame time is NOT the battery story — the hybrid path trades GPU\n  \
         compute for CPU preprocessing, so watts can win even when ms are close.\n  \
         Compare PNGs for visual parity; measure WATTS with the commands below.\n"
    );
    Ok(())
}

/// Phase-4 verification: build a Pebbles scene through the framework's real `Painter`
/// (the op-list recorder), `flush` it into a `vello_hybrid::Scene`, and rasterize — the
/// exact path the shell host will drive. Proves the op→backend translation renders.
fn flush_demo(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<(), String> {
    use pebbles_render::paint::{Painter, Scene as PScene};

    let (texture, view) = offscreen(
        device,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let (mut renderer, mut resources) = HRenderer::new(
        device,
        &RenderTargetConfig {
            format: texture.format(),
            width: W as u32,
            height: H as u32,
        },
    );

    // Record through the framework Painter (identical vocabulary — same kurbo/peniko).
    let mut pscene = PScene::new();
    {
        let mut p = Painter::new(&mut pscene);
        paint_pebbles(&mut p);
    }
    // Flush the recorded op-list into a live backend scene, then render it.
    let mut hyb = HScene::new(W, H);
    pscene.flush(&mut hyb, &mut resources);

    // Bind the external textures the flush references (mirrors the shell's hybrid host).
    let uploads = pscene.image_uploads();
    println!("  flush image_uploads: {} image(s)", uploads.len());
    let mut bindings = TextureBindings::new();
    let mut _keep_alive = Vec::new();
    for (i, up) in uploads.iter().enumerate() {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("flush.image"),
            size: wgpu::Extent3d {
                width: up.width,
                height: up.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &up.rgba8,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(up.width * 4),
                rows_per_image: Some(up.height),
            },
            wgpu::Extent3d {
                width: up.width,
                height: up.height,
                depth_or_array_layers: 1,
            },
        );
        bindings.insert(vello_hybrid::TextureId(i as u64), tex.create_view(&Default::default()));
        _keep_alive.push(tex);
    }

    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    renderer
        .render(
            &hyb,
            &mut resources,
            device,
            queue,
            &mut encoder,
            &RenderSize {
                width: W as u32,
                height: H as u32,
            },
            &view,
            &bindings,
        )
        .map_err(|e| format!("flush render: {e:?}"))?;
    queue.submit([encoder.finish()]);
    device.poll(wgpu::PollType::wait_indefinitely()).ok();
    save_png(device, queue, &texture, "flush.png");
    Ok(())
}

/// The representative dashboard, recorded through the framework `Painter` (builder-style,
/// same signatures as vello). Mirrors `paint_vello` so `flush.png` should match `vello.png`
/// / `hybrid.png` — proving the flush faithfully reproduces the scene.
/// A 64×64 RGBA checkerboard-with-gradient test image (proves the external-texture path).
fn test_image() -> peniko::ImageBrush {
    let (w, h) = (64u32, 64u32);
    let mut data = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let checker = if ((x / 8) + (y / 8)) % 2 == 0 { 255 } else { 40 };
            data[i] = checker;
            data[i + 1] = (x * 4) as u8;
            data[i + 2] = (y * 4) as u8;
            data[i + 3] = 255;
        }
    }
    peniko::ImageBrush {
        image: peniko::ImageData {
            data: peniko::Blob::from(data),
            format: peniko::ImageFormat::Rgba8,
            width: w,
            height: h,
            alpha_type: peniko::ImageAlphaType::Alpha,
        },
        sampler: peniko::ImageSampler::default(),
    }
}

fn paint_pebbles(p: &mut pebbles_render::paint::Painter) {
    let id = Affine::IDENTITY;
    p.fill(Fill::NonZero, id, &Brush::Solid(Color::from_rgb8(15, 17, 26)), None, &Rect::new(0.0, 0.0, W as f64, H as f64));
    p.fill(Fill::NonZero, id, &Brush::Gradient(hero_gradient()), None, &Rect::new(0.0, 0.0, W as f64, 220.0));

    for r in card_rects() {
        p.draw_blurred_rounded_rect(id, r, Color::from_rgba8(0, 0, 0, 90), 16.0, 14.0);
        p.fill(Fill::NonZero, id, &Brush::Solid(Color::from_rgb8(28, 31, 44)), None, &RoundedRect::from_rect(r, 16.0));
        p.push_layer(Fill::NonZero, Mix::Normal, 1.0, id, &RoundedRect::from_rect(r, 16.0));
        p.fill(Fill::NonZero, id, &Brush::Gradient(accent_gradient()), None, &Rect::new(r.x0, r.y0, r.x1, r.y0 + 40.0));
        p.pop_layer();
        p.stroke(&Stroke::new(1.0), id, &Brush::Solid(Color::from_rgba8(255, 255, 255, 24)), None, &RoundedRect::from_rect(r, 16.0));
    }

    // ShaderMask (DestIn emulation) via the Painter
    let mb = Rect::new(48.0, 620.0, 560.0, 680.0);
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, id, &mb);
    p.fill(Fill::NonZero, id, &Brush::Solid(Color::from_rgb8(236, 72, 153)), None, &mb);
    p.push_layer(Fill::NonZero, BlendMode::new(Mix::Normal, Compose::DestIn), 1.0, id, &mb);
    p.fill(Fill::NonZero, id, &Brush::Gradient(fade_over(mb)), None, &mb);
    p.pop_layer();
    p.pop_layer();

    p.push_layer(Fill::NonZero, Mix::Normal, 0.85, id, &Rect::new(48.0, 700.0, W as f64 - 48.0, 760.0));
    p.fill(Fill::NonZero, id, &Brush::Gradient(accent_gradient()), None, &Rect::new(48.0, 700.0, W as f64 - 48.0, 760.0));
    p.pop_layer();

    // A drawn image (external texture): 64×64 test image scaled to 160px at top-right.
    let img = test_image();
    p.draw_image(&img, Affine::translate((900.0, 40.0)) * Affine::scale(160.0 / 64.0));
}

fn time_vello(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<f64, String> {
    let mut renderer = VRenderer::new(
        device,
        RendererOptions {
            use_cpu: false,
            antialiasing_support: vello::AaSupport::area_only(),
            num_init_threads: None,
            pipeline_cache: None,
        },
    )
    .map_err(|e| format!("vello Renderer::new: {e:?}"))?;

    // Full Vello's compute rasterizer writes the final image through a STORAGE
    // binding — the target MUST carry STORAGE_BINDING (RENDER_ATTACHMENT is not
    // required). This differs from the hybrid path below, which is a raster pass.
    let (texture, view) = offscreen(
        device,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
    );

    let mut scene = VScene::new();
    let mut last = 0.0;
    for i in 0..FRAMES {
        scene.reset();
        paint_vello(&mut scene);
        let t = Instant::now();
        renderer
            .render_to_texture(
                device,
                queue,
                &scene,
                &view,
                &RenderParams {
                    base_color: Color::from_rgb8(15, 17, 26),
                    width: W as u32,
                    height: H as u32,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .map_err(|e| format!("vello render_to_texture: {e:?}"))?;
        device.poll(wgpu::PollType::wait_indefinitely()).ok();
        last = t.elapsed().as_secs_f64() * 1000.0;
        if i == FRAMES - 1 {
            save_png(device, queue, &texture, "vello.png");
        }
    }
    Ok(last)
}

fn time_hybrid(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<f64, String> {
    // The hybrid path is a raster pass — its target is a RENDER_ATTACHMENT
    // (no STORAGE_BINDING), the opposite of full Vello's compute target above.
    let (texture, view) = offscreen(
        device,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let (mut renderer, mut resources) = HRenderer::new(
        device,
        &RenderTargetConfig {
            format: texture.format(),
            width: W as u32,
            height: H as u32,
        },
    );
    let render_size = RenderSize {
        width: W as u32,
        height: H as u32,
    };

    let mut last = 0.0;
    for i in 0..FRAMES {
        let mut scene = HScene::new(W, H);
        paint_hybrid(&mut scene);
        let mut encoder =
            device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let t = Instant::now();
        renderer
            .render(
                &scene,
                &mut resources,
                device,
                queue,
                &mut encoder,
                &render_size,
                &view,
                &TextureBindings::new(),
            )
            .map_err(|e| format!("hybrid render: {e:?}"))?;
        queue.submit([encoder.finish()]);
        device.poll(wgpu::PollType::wait_indefinitely()).ok();
        last = t.elapsed().as_secs_f64() * 1000.0;
        if i == FRAMES - 1 {
            save_png(device, queue, &texture, "hybrid.png");
        }
    }
    Ok(last)
}

// ---------------------------------------------------------------------------
// The representative scene — the same Pebbles-ish dashboard on both backends
// ---------------------------------------------------------------------------

/// Six cards in a 3×2 grid over a gradient hero band, each with a soft drop
/// shadow, a colored header stripe (clipped to the rounded top), a body fill,
/// and a hairline border — plus a translucent bottom action bar. This mirrors
/// what a real Pebbles screen emits: fills, rounded paths, strokes, gradients,
/// blurred-rect shadows, clip + opacity layers.
fn card_rects() -> Vec<Rect> {
    let mut v = Vec::new();
    let (cols, rows) = (3, 2);
    let (mx, my, gap) = (48.0, 260.0, 32.0);
    let cw = (W as f64 - 2.0 * mx - (cols as f64 - 1.0) * gap) / cols as f64;
    let ch = 150.0;
    for r in 0..rows {
        for c in 0..cols {
            let x = mx + c as f64 * (cw + gap);
            let y = my + r as f64 * (ch + gap);
            v.push(Rect::new(x, y, x + cw, y + ch));
        }
    }
    v
}

fn card_path(r: Rect) -> BezPath {
    RoundedRect::from_rect(r, 16.0).to_path(0.1)
}

fn hero_gradient() -> Gradient {
    Gradient::new_linear(Point::new(0.0, 0.0), Point::new(W as f64, 220.0))
        .with_stops([Color::from_rgb8(99, 102, 241), Color::from_rgb8(236, 72, 153)].as_slice())
}

fn accent_gradient() -> Gradient {
    Gradient::new_linear(Point::new(0.0, 0.0), Point::new(W as f64, 0.0))
        .with_stops([Color::from_rgb8(16, 185, 129), Color::from_rgb8(59, 130, 246)].as_slice())
}

/// A white ramp whose ALPHA fades 1→0 left-to-right across `rect` — the derived mask
/// gradient the ShaderMask emulation composites with `DestIn` (alpha = luminance).
fn fade_over(r: Rect) -> Gradient {
    Gradient::new_linear(Point::new(r.x0, 0.0), Point::new(r.x1, 0.0)).with_stops(
        [
            Color::from_rgba8(255, 255, 255, 255),
            Color::from_rgba8(255, 255, 255, 0),
        ]
        .as_slice(),
    )
}

/// A vertical white alpha ramp 1→0 over 120px (used by the coverage probe).
fn luminance_fade() -> Gradient {
    Gradient::new_linear(Point::new(0.0, 0.0), Point::new(0.0, 120.0)).with_stops(
        [
            Color::from_rgba8(255, 255, 255, 255),
            Color::from_rgba8(255, 255, 255, 0),
        ]
        .as_slice(),
    )
}

fn glow_gradient() -> Gradient {
    Gradient::new_radial(Point::new(150.0, 150.0), 150.0).with_stops(
        [
            Color::from_rgba8(255, 255, 255, 90),
            Color::from_rgba8(255, 255, 255, 0),
        ]
        .as_slice(),
    )
}

fn paint_hybrid(s: &mut HScene) {
    // background
    s.set_paint(Color::from_rgb8(15, 17, 26));
    s.fill_rect(&Rect::new(0.0, 0.0, W as f64, H as f64));
    // hero gradient band
    s.set_paint(hero_gradient());
    s.fill_rect(&Rect::new(0.0, 0.0, W as f64, 220.0));

    for r in card_rects() {
        // drop shadow
        s.set_paint(Color::from_rgba8(0, 0, 0, 90));
        s.fill_blurred_rounded_rect(&r, 16.0, 14.0, false);
        // body
        s.set_paint(Color::from_rgb8(28, 31, 44));
        s.fill_path(&card_path(r));
        // header stripe, clipped to the rounded card
        s.push_clip_layer(&card_path(r));
        s.set_paint(accent_gradient());
        s.fill_rect(&Rect::new(r.x0, r.y0, r.x1, r.y0 + 40.0));
        s.pop_layer();
        // hairline border
        s.set_stroke(Stroke::new(1.0));
        s.set_paint(Color::from_rgba8(255, 255, 255, 24));
        s.stroke_path(&card_path(r));
    }

    // ShaderMask (DestIn emulation): a magenta panel faded out to the right
    let mb = Rect::new(48.0, 620.0, 560.0, 680.0);
    let mclip = mb.to_path(0.1);
    s.push_layer(Some(&mclip), None, None, None, None); // isolate
    s.set_paint(Color::from_rgb8(236, 72, 153));
    s.fill_rect(&mb);
    s.push_layer(
        Some(&mclip),
        Some(BlendMode::new(Mix::Normal, Compose::DestIn)),
        None,
        None,
        None,
    );
    s.set_paint(fade_over(mb));
    s.fill_rect(&mb);
    s.pop_layer();
    s.pop_layer();

    // translucent bottom action bar
    s.push_opacity_layer(0.85);
    s.set_paint(accent_gradient());
    s.fill_rect(&Rect::new(48.0, 700.0, W as f64 - 48.0, 760.0));
    s.pop_layer();
}

fn paint_vello(s: &mut VScene) {
    let id = Affine::IDENTITY;
    s.fill(
        Fill::NonZero,
        id,
        &Brush::Solid(Color::from_rgb8(15, 17, 26)),
        None,
        &Rect::new(0.0, 0.0, W as f64, H as f64),
    );
    s.fill(
        Fill::NonZero,
        id,
        &Brush::Gradient(hero_gradient()),
        None,
        &Rect::new(0.0, 0.0, W as f64, 220.0),
    );

    for r in card_rects() {
        s.draw_blurred_rounded_rect(id, r, Color::from_rgba8(0, 0, 0, 90), 16.0, 14.0);
        s.fill(
            Fill::NonZero,
            id,
            &Brush::Solid(Color::from_rgb8(28, 31, 44)),
            None,
            &RoundedRect::from_rect(r, 16.0),
        );
        // header stripe clipped to the rounded card
        s.push_layer(Fill::NonZero, Mix::Normal, 1.0, id, &RoundedRect::from_rect(r, 16.0));
        s.fill(
            Fill::NonZero,
            id,
            &Brush::Gradient(accent_gradient()),
            None,
            &Rect::new(r.x0, r.y0, r.x1, r.y0 + 40.0),
        );
        s.pop_layer();
        s.stroke(
            &Stroke::new(1.0),
            id,
            &Brush::Solid(Color::from_rgba8(255, 255, 255, 24)),
            None,
            &RoundedRect::from_rect(r, 16.0),
        );
    }

    // ShaderMask (DestIn emulation) — the full-Vello reference for the hybrid panel.
    let mb = Rect::new(48.0, 620.0, 560.0, 680.0);
    s.push_layer(Fill::NonZero, Mix::Normal, 1.0, id, &mb); // isolate
    s.fill(Fill::NonZero, id, &Brush::Solid(Color::from_rgb8(236, 72, 153)), None, &mb);
    s.push_layer(Fill::NonZero, BlendMode::new(Mix::Normal, Compose::DestIn), 1.0, id, &mb);
    s.fill(Fill::NonZero, id, &Brush::Gradient(fade_over(mb)), None, &mb);
    s.pop_layer();
    s.pop_layer();

    s.push_layer(
        Fill::NonZero,
        Mix::Normal,
        0.85,
        id,
        &Rect::new(48.0, 700.0, W as f64 - 48.0, 760.0),
    );
    s.fill(
        Fill::NonZero,
        id,
        &Brush::Gradient(accent_gradient()),
        None,
        &Rect::new(48.0, 700.0, W as f64 - 48.0, 760.0),
    );
    s.pop_layer();
}

// ---------------------------------------------------------------------------
// wgpu offscreen target + PNG readback (shared by both backends)
// ---------------------------------------------------------------------------

fn offscreen(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen"),
        size: wgpu::Extent3d {
            width: W as u32,
            height: H as u32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn save_png(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture, path: &str) {
    let bytes_per_row = (W as u32 * 4).next_multiple_of(256);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: bytes_per_row as u64 * H as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: W as u32,
            height: H as u32,
            depth_or_array_layers: 1,
        },
    );
    queue.submit([encoder.finish()]);
    buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).ok();

    let mut rgba = Vec::with_capacity(W as usize * H as usize * 4);
    for row in buffer
        .slice(..)
        .get_mapped_range()
        .chunks_exact(bytes_per_row as usize)
    {
        rgba.extend_from_slice(&row[..W as usize * 4]);
    }
    buffer.unmap();

    let file = std::fs::File::create(path).expect("create png");
    let w = std::io::BufWriter::new(file);
    let mut enc = png::Encoder::new(w, W as u32, H as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&rgba).unwrap();
}

// ---------------------------------------------------------------------------
// Watt-capture recipes (the operator runs these on real hardware)
// ---------------------------------------------------------------------------

fn print_watt_commands() {
    println!("── On-hardware watt capture (run each while looping the render) ──");
    println!("  macOS (Apple Silicon):  sudo powermetrics --samplers gpu_power -i 200");
    println!("  Linux / Intel iGPU:     sudo intel_gpu_top");
    println!("  Linux / NVIDIA:         nvidia-smi --query-gpu=power.draw --format=csv -l 1");
    println!("  Linux / AMD:            watch -n1 'cat /sys/class/drm/card*/device/hwmon/hwmon*/power1_average'");
    println!("  Battery drain (any):    upower -i $(upower -e | grep BAT) | grep energy-rate");
    println!();
    println!("  Compare: same scene, `--features`-selected backend, on battery, screen");
    println!("  fixed brightness. The hybrid path should draw fewer GPU watts at the");
    println!("  same visual result — that delta is the whole reason it's the default.");
}
