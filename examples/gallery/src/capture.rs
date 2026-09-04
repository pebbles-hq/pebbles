//! Headless demo capture for the two-window IPC story (pre-competition closer).
//!
//! `GALLERY_CAPTURE=<dir> cargo run -p gallery --release` renders the REAL
//! "Windows & IPC" screen and its secondary counter window off-screen — no
//! display server, no screenshot tool, no consent dialog — driving the actual
//! shared signal + typed channel and rasterizing each window through vello to a
//! GPU texture it reads back to raw RGBA. `demo/build_demo.py` then composites
//! the two windows per step into the screenshot strip and the animated GIF.
//!
//! Why headless rather than a screen grab: it captures the exact rendered
//! pixels, needs nothing installed, and is fully reproducible in CI — and it
//! proves the IPC at the model level (the secondary window updates *because* the
//! shared signal / channel propagated, not because a human clicked).

use std::io::Write;
use std::path::Path;

use pebbles::core::Ui;
use pebbles::prelude::*;
use pebbles::render::{Scene, TextEnv};
use vello::util::RenderContext;
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions};
use wgpu::{
    Extent3d, TexelCopyBufferInfo, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureDescriptor,
    TextureDimension, TextureFormat, TextureUsages,
};

use crate::screens::windows::{counter_window, windows};
use crate::state;

const MAIN_W: u32 = 780;
const MAIN_H: u32 = 620;
const SIDE_W: u32 = 360;
const SIDE_H: u32 = 280;

/// One scripted beat of the demo: a caption + the model mutation that drives it.
/// After each, BOTH windows are re-rendered — the secondary reflects the change
/// with zero serialization, which is the whole point.
struct Beat {
    caption: &'static str,
    act: fn(),
}

/// The GPU side: a headless vello renderer plus a reused readback scratch buffer.
struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
}

impl Gpu {
    fn new() -> Self {
        // vello's own RenderContext requests an adapter + device with exactly the
        // features/limits vello needs — the same path the shell uses, minus a
        // surface (headless).
        let mut ctx = RenderContext::new();
        let dev_id = pollster::block_on(ctx.device(None)).expect("no compatible GPU device");
        let handle = ctx.devices.remove(dev_id);
        let renderer = Renderer::new(
            &handle.device,
            RendererOptions {
                use_cpu: false,
                antialiasing_support: AaSupport::area_only(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .expect("vello renderer");
        Gpu { device: handle.device, queue: handle.queue, renderer }
    }

    /// Rasterize `scene` at `w×h` on a white base and read the pixels back as
    /// tight (unpadded) RGBA8.
    fn rasterize(&mut self, scene: &Scene, w: u32, h: u32) -> Vec<u8> {
        let texture = self.device.create_texture(&TextureDescriptor {
            label: Some("capture-target"),
            size: Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::STORAGE_BINDING | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.renderer
            .render_to_texture(
                &self.device,
                &self.queue,
                scene,
                &view,
                &RenderParams {
                    base_color: vello::peniko::Color::from_rgba8(255, 255, 255, 255),
                    width: w,
                    height: h,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .expect("render_to_texture");

        // Copy the texture into a buffer, honoring wgpu's 256-byte row alignment.
        let unpadded = w * 4;
        let padded = unpadded.div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture-readback"),
            size: u64::from(padded) * u64::from(h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        enc.copy_texture_to_buffer(
            TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            TexelCopyBufferInfo {
                buffer: &buffer,
                layout: TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit([enc.finish()]);

        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely()).expect("poll");
        rx.recv().expect("map channel").expect("buffer mapped");

        let mapped = slice.get_mapped_range();
        let mut out = Vec::with_capacity((unpadded * h) as usize);
        for row in 0..h {
            let start = (row * padded) as usize;
            out.extend_from_slice(&mapped[start..start + unpadded as usize]);
        }
        drop(mapped);
        buffer.unmap();
        out
    }
}

/// Rebuild + lay out + paint a window to a `Scene`, settling any P5.2 corrective
/// relayout (bounded — these screens have at most a single-line field).
fn scene_for(ui: &mut Ui, env: &mut TextEnv, w: u32, h: u32) -> Scene {
    let size = Size::new(f64::from(w), f64::from(h));
    ui.make_current();
    let mut scene = Scene::new();
    for _ in 0..4 {
        ui.rebuild_if_dirty();
        ui.layout(env, size);
        scene = Scene::new();
        if !ui.paint(env, &mut scene) {
            break;
        }
    }
    scene
}

/// Write tight RGBA to `<dir>/<name>.rgba` and append `name w h` to the manifest.
fn dump(dir: &Path, manifest: &mut std::fs::File, name: &str, px: &[u8], w: u32, h: u32) {
    std::fs::write(dir.join(format!("{name}.rgba")), px).expect("write frame");
    writeln!(manifest, "{name} {w} {h}").expect("write manifest");
}

/// Entry point when `GALLERY_CAPTURE=<dir>` is set. Returns after writing every
/// frame + a manifest; never opens a window.
pub fn run(out_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    let dir = Path::new(out_dir);
    std::fs::create_dir_all(dir)?;
    eprintln!("gallery capture → headless GPU, writing frames to {out_dir}");

    Theme::light().make_current();
    pebbles::widgets::overlay::init();
    pebbles::core::focus::init();
    // Global app-scope state, created BEFORE any window so it is shared (the IPC
    // substrate) rather than owned by one window's tree.
    state::init();

    let mut gpu = Gpu::new();
    let mut env = TextEnv::new();

    // Two real windows, each its own `Ui`, sharing the one runtime — exactly the
    // shell's per-window wrap (`View` + `OverlayHost`).
    let bg = theme().colors.background;
    let mut main = Ui::new();
    main.make_current();
    main.mount_root(View::new(bg, OverlayHost::wrap(component(windows).into_widget())).into_widget());
    let mut side = Ui::new();
    side.make_current();
    side.mount_root(View::new(bg, OverlayHost::wrap(component(counter_window).into_widget())).into_widget());

    // The script: increment the shared counter, send typed messages — the beats
    // that show live cross-window sync.
    let beats = [
        Beat { caption: "A second OS window opens, sharing the runtime", act: || {} },
        Beat { caption: "+1 in the main window", act: || state::counter().update(|c| *c += 1) },
        Beat {
            caption: "+1 again — the other window already agrees",
            act: || state::counter().update(|c| *c += 1),
        },
        Beat {
            caption: "A typed message crosses the channel",
            act: || state::ping().send("Hello from the main window".into()),
        },
        Beat {
            caption: "+1 — no serialization, just a shared signal",
            act: || state::counter().update(|c| *c += 1),
        },
        Beat {
            caption: "Another message, delivered instantly",
            act: || state::ping().send("State syncs live across windows".into()),
        },
        Beat { caption: "+1 — both windows in lockstep", act: || state::counter().update(|c| *c += 1) },
    ];

    let mut manifest = std::fs::File::create(dir.join("manifest.txt"))?;
    for (i, beat) in beats.iter().enumerate() {
        (beat.act)();
        // Re-render BOTH windows so the secondary reflects the shared change.
        let main_px = {
            let s = scene_for(&mut main, &mut env, MAIN_W, MAIN_H);
            gpu.rasterize(&s, MAIN_W, MAIN_H)
        };
        let side_px = {
            let s = scene_for(&mut side, &mut env, SIDE_W, SIDE_H);
            gpu.rasterize(&s, SIDE_W, SIDE_H)
        };
        dump(dir, &mut manifest, &format!("step{i}_main"), &main_px, MAIN_W, MAIN_H);
        dump(dir, &mut manifest, &format!("step{i}_side"), &side_px, SIDE_W, SIDE_H);
        writeln!(manifest, "# step {i}: {}", beat.caption)?;
        env.finish_frame();
        eprintln!("  step {i}: {}", beat.caption);
    }
    eprintln!("gallery capture → {} steps written", beats.len());
    Ok(())
}
