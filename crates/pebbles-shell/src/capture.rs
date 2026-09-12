//! Offscreen capture — render any widget tree to a PNG (or raw RGBA) with no window or
//! display. This is the framework's "export" primitive: a widget can't rasterize *itself*
//! (it has no GPU handle), but the shell can render one headlessly, exactly as the live
//! runner does, and read the pixels back. Handy for chart/report export, server-side
//! rendering, and golden-image tests.
//!
//! Desktop + the default `vello-hybrid` backend only (it needs an offscreen GPU device).
//!
//! ```ignore
//! use pebbles_shell::capture;
//! let png = capture::capture_png(my_chart().into_widget(), 800, 400, palette::WHITE)?;
//! std::fs::write("chart.png", png)?;
//! ```

use std::error::Error;

use pebbles_core::{AnyWidget, IntoWidget, Ui};
use pebbles_foundation::{Color, Size};
use pebbles_render::paint::kurbo;
use pebbles_render::{Scene, TextEnv};
use pebbles_widgets::{OverlayHost, View};
use vello::util::RenderContext;
use vello_hybrid::{
    RenderSize, RenderTargetConfig, Renderer as HybRenderer, Resources, Scene as HybScene, TextureBindings,
};
use wgpu::{
    Extent3d, TexelCopyBufferInfo, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureDescriptor,
    TextureDimension, TextureFormat, TextureUsages,
};

/// Render `root` at `width`×`height` (logical px) on `background` and return raw, tightly
/// packed **unmultiplied RGBA8** (row-major, `width*height*4` bytes). See [`capture_png`]
/// for an encoded PNG.
///
/// Animations are settled to their final frame first, so entry/transition animations don't
/// leave a half-drawn capture.
pub fn capture_rgba(
    root: AnyWidget,
    width: u32,
    height: u32,
    background: Color,
) -> Result<Vec<u8>, Box<dyn Error>> {
    if width == 0 || height == 0 {
        return Err("capture size must be non-zero".into());
    }
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut gpu = Gpu::new(width, height)?;
    let mut env = TextEnv::new();
    let mut ui = Ui::new();
    ui.make_current();
    ui.mount_root(View::new(background, OverlayHost::wrap(root)).into_widget());
    pebbles_widgets::overlay::set_window_size(f64::from(width), f64::from(height));

    let scene = scene_for(&mut ui, &mut env, width, height);
    Ok(gpu.rasterize(&scene, width, height, background))
}

/// Render `root` at `width`×`height` on `background` and return PNG bytes.
pub fn capture_png(
    root: AnyWidget,
    width: u32,
    height: u32,
    background: Color,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let rgba = capture_rgba(root, width, height, background)?;
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()?.write_image_data(&rgba)?;
    }
    Ok(out)
}

fn scene_for(ui: &mut Ui, env: &mut TextEnv, w: u32, h: u32) -> Scene {
    let size = Size::new(f64::from(w), f64::from(h));
    ui.make_current();
    // Settle animations: jump the clock far ahead so entry/transition tweens finish, then
    // paint the resulting static frame.
    let mut scene = Scene::new();
    for i in 0..6 {
        pebbles_core::animation::tick(1000.0 + f64::from(i));
        ui.rebuild_if_dirty();
        ui.layout(env, size);
        scene = Scene::new();
        if !ui.paint(env, &mut scene) {
            break;
        }
    }
    scene
}

struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: HybRenderer,
    resources: Resources,
}

impl Gpu {
    fn new(w: u32, h: u32) -> Result<Self, Box<dyn Error>> {
        let mut ctx = RenderContext::new();
        let dev_id =
            pollster::block_on(ctx.device(None)).ok_or("no compatible GPU device for capture")?;
        let handle = ctx.devices.remove(dev_id);
        let (renderer, resources) = HybRenderer::new(
            &handle.device,
            &RenderTargetConfig { format: TextureFormat::Rgba8Unorm, width: w, height: h },
        );
        Ok(Gpu { device: handle.device, queue: handle.queue, renderer, resources })
    }

    fn rasterize(&mut self, scene: &Scene, w: u32, h: u32, base: Color) -> Vec<u8> {
        let texture = self.device.create_texture(&TextureDescriptor {
            label: Some("capture"),
            size: Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut hyb = HybScene::new(w.min(u32::from(u16::MAX)) as u16, h.min(u32::from(u16::MAX)) as u16);
        let [r, g, b, _] = base.components;
        hyb.set_paint(Color::new([r, g, b, 1.0]));
        hyb.fill_rect(&kurbo::Rect::new(0.0, 0.0, f64::from(w), f64::from(h)));
        scene.flush(&mut hyb, &mut self.resources);
        let bindings = TextureBindings::new();

        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        self.renderer
            .render(
                &hyb,
                &mut self.resources,
                &self.device,
                &self.queue,
                &mut enc,
                &RenderSize { width: w, height: h },
                &view,
                &bindings,
            )
            .expect("capture render");
        self.queue.submit([enc.finish()]);

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
                layout: TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) },
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
        rx.recv().expect("map").expect("mapped");
        let mapped = slice.get_mapped_range();
        let mut out = Vec::with_capacity((unpadded * h) as usize);
        for row in 0..h {
            let s = (row * padded) as usize;
            out.extend_from_slice(&mapped[s..s + unpadded as usize]);
        }
        drop(mapped);
        buffer.unmap();
        out
    }
}
