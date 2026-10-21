//! Offscreen capture — render any widget tree to a PNG (or raw RGBA) with no window or
//! display. This is the framework's "export" primitive: a widget can't rasterize *itself*
//! (it has no GPU handle), but the shell can render one headlessly, exactly as the live
//! runner does, and read the pixels back. Handy for chart/report export, server-side
//! rendering, and golden-image tests.
//!
//! Desktop only: it needs an offscreen GPU device, and the web has no headless device. Works
//! under whichever render backend the build selects. [`OffscreenCapture`] keeps one device for
//! many frames; [`capture_rgba`]/[`capture_png`] create one per call.
//!
//! Dimensions are capped at [`MAX_CAPTURE_DIM`]² — an unbounded size asks the GPU for a
//! multi-gigabyte texture + readback buffer, a memory-exhaustion vector (relevant for the
//! documented server-side-rendering use). GPU failures return `Err`, never panic. Both
//! functions take over the thread-local *current* `Ui` while they run, so do **not** call
//! them from a live app's UI thread — use a background thread or a dedicated headless process.
//!
//! ```no_run
//! use pebbles_core::IntoWidget;
//! use pebbles_foundation::palette;
//! use pebbles_shell::capture;
//! use pebbles_widgets::text;
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let png = capture::capture_png(text("chart").into_widget(), 800, 400, palette::WHITE)?;
//!     std::fs::write("chart.png", png)?;
//!     Ok(())
//! }
//! ```

use std::error::Error;
use std::time::Duration;

use pebbles_core::{AnyWidget, IntoWidget, Ui, prefers_reduced_motion, set_prefers_reduced_motion};
use pebbles_foundation::{Color, Size};
use pebbles_render::{Scene, TextEnv};
use pebbles_widgets::{OverlayHost, View};
#[cfg(feature = "vello-hybrid")]
use wgpu::{
    Extent3d, TexelCopyBufferInfo, TexelCopyBufferLayout, TexelCopyTextureInfo, TextureDescriptor,
    TextureDimension, TextureFormat, TextureUsages,
};

use crate::gpu;
#[cfg(feature = "vello-hybrid")]
use crate::gpu::RenderContext;

/// Maximum capture dimension per side. A capture allocates a `width×height×4` GPU texture
/// plus a 256-byte-row-padded readback buffer, so an unbounded size is a memory-exhaustion
/// vector; this ceiling also stays within the max 2D texture size of common GPUs and keeps
/// the readback offset math well inside `u32`.
pub const MAX_CAPTURE_DIM: u32 = 16_384;

/// Upper bound on how long the readback poll waits for the GPU before returning `Err`. A
/// bounded wait (vs. `PollType::wait_indefinitely`) keeps a wedged or lost driver from
/// hanging the calling thread forever; the round-trip normally completes in milliseconds.
pub const GPU_POLL_TIMEOUT: Duration = Duration::from_secs(10);

/// Restores the global reduced-motion preference on drop, so a `?` early-return or a panic
/// mid-capture can't leave the process permanently stuck in reduced-motion.
struct ReducedMotionGuard(bool);
impl Drop for ReducedMotionGuard {
    fn drop(&mut self) {
        set_prefers_reduced_motion(self.0);
    }
}

/// Render `root` at `width`×`height` (logical px) on `background` and return raw, tightly
/// packed **unmultiplied RGBA8** (row-major, `width*height*4` bytes). See [`capture_png`]
/// for an encoded PNG.
///
/// Animations are settled to their final frame first, so entry/transition animations don't
/// leave a half-drawn capture.
///
/// # Blocking
///
/// This call **blocks the calling thread for a full GPU round-trip** (submit + map + readback)
/// — potentially tens of milliseconds, and up to [`GPU_POLL_TIMEOUT`] on a stalled driver
/// before it returns `Err`. Do **not** call it from a hot event handler or the live UI thread;
/// use a background thread or a dedicated headless process (see the module docs).
pub fn capture_rgba(
    root: AnyWidget,
    width: u32,
    height: u32,
    background: Color,
) -> Result<Vec<u8>, Box<dyn Error>> {
    if width == 0 || height == 0 {
        return Err("capture size must be non-zero".into());
    }
    if width > MAX_CAPTURE_DIM || height > MAX_CAPTURE_DIM {
        return Err(
            format!("capture size {width}×{height} exceeds the {MAX_CAPTURE_DIM}px per-side limit").into()
        );
    }
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    // A capture is one still frame — there's no clock to run entry/transition animations,
    // so render the settled (reduced-motion) state: mount-time animations resolve to their
    // final frame instead of the blank first frame. The guard restores the prior preference
    // on EVERY exit path (early `?` return or a panic), not just success.
    let _reduced = ReducedMotionGuard(prefers_reduced_motion());
    set_prefers_reduced_motion(true);

    let mut gpu = OffscreenCapture::new()?;
    // One environment per call (never cached, 03 §3.1); system fonts scanned inline so a
    // screenshot matches the app.
    let mut env = TextEnv::with_system_fonts(pebbles_render::SystemFonts::Blocking);
    let mut ui = Ui::new();
    ui.make_current();
    ui.mount_root(View::new(background, OverlayHost::wrap(root)).into_widget());
    pebbles_widgets::overlay::set_window_size(f64::from(width), f64::from(height));

    let scene = scene_for(&mut ui, &mut env, width, height);
    Ok(gpu.render_scene(&scene, width, height, background)?)
}

/// Render `root` at `width`×`height` on `background` and return PNG bytes.
///
/// # Blocking
///
/// Like [`capture_rgba`] (which it wraps), this **blocks the calling thread for a full GPU
/// round-trip** and PNG encode. Don't call it from a hot event handler or the live UI thread.
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

#[cfg(feature = "vello-hybrid")]
struct HybridCapture {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: gpu::Renderer,
}

#[cfg(feature = "vello-hybrid")]
impl HybridCapture {
    fn new() -> Result<Self, CaptureError> {
        // Acquire an offscreen device through the SAME `gpu/` host seam the runner uses — no
        // second wgpu path. The renderer is built lazily on the first `render_offscreen`.
        {
            let mut ctx = RenderContext::new();
            let dev_id = pollster::block_on(ctx.device(None))
                // The same typed error as the wgpu path (DR-12), so callers can tell "no GPU" apart.
                .ok_or(CaptureError::Gpu(pebbles_gpu::GpuError::NoAdapter {
                    backends: wgpu::Backends::all(),
                    software_only: false,
                    enumerated: 0,
                    source: None,
                }))?;
            let handle = ctx.devices.remove(dev_id);
            let renderer = gpu::new_renderer(&handle.device, &handle.queue);
            Ok(HybridCapture { device: handle.device, queue: handle.queue, renderer })
        }
    }

    fn render(&mut self, scene: &Scene, w: u32, h: u32, base: Color) -> Result<Vec<u8>, CaptureError> {
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

        // Force an opaque background, then render through the hybrid host's shared entry point
        // — this reuses the runner's flush + `draw_image` binding, so captured widgets that
        // contain images rasterize correctly (the old bespoke path bound no image textures).
        let [r, g, b, _] = base.components;
        let opaque = Color::new([r, g, b, 1.0]);
        self.renderer
            .render_offscreen(
                &self.device,
                &self.queue,
                scene,
                &view,
                opaque,
                (w, h),
                TextureFormat::Rgba8Unorm,
            )
            .map_err(|e| CaptureError::Backend(format!("render failed: {e}")))?;

        // Offset math in `usize` (the `MAX_CAPTURE_DIM` cap already keeps it inside `u32`, but
        // this is the readback that would OOB-panic on a wrapped index — be explicit).
        let unpadded = w as usize * 4;
        let padded = unpadded.div_ceil(256) * 256;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("capture-readback"),
            size: (padded * h as usize) as u64,
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
                    bytes_per_row: Some(padded as u32),
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
        // Bounded wait: a wedged/lost driver returns `PollError::Timeout` instead of hanging
        // the calling thread forever (an indefinite wait is the UI-freeze vector documented on
        // the public entry points).
        self.device
            .poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(GPU_POLL_TIMEOUT) })
            .map_err(CaptureError::Poll)?;
        rx.recv().map_err(|_| CaptureError::ReadbackAborted)?.map_err(CaptureError::Map)?;
        let mapped = slice.get_mapped_range();
        let mut out = Vec::with_capacity(unpadded * h as usize);
        for row in 0..h as usize {
            let s = row * padded;
            out.extend_from_slice(&mapped[s..s + unpadded]);
        }
        drop(mapped);
        buffer.unmap();
        Ok(out)
    }
}

/// The wgpu backend's inner renderer: pebbles-gpu's public headless renderer (C15).
#[cfg(all(feature = "wgpu", not(feature = "vello-hybrid")))]
struct WgpuCapture(OffscreenRenderer);

#[cfg(all(feature = "wgpu", not(feature = "vello-hybrid")))]
impl WgpuCapture {
    fn new() -> Result<Self, CaptureError> {
        Ok(WgpuCapture(OffscreenRenderer::with_init(gpu::HostInit::headless().gpu)?))
    }

    fn render(&mut self, scene: &Scene, w: u32, h: u32, base: Color) -> Result<Vec<u8>, CaptureError> {
        Ok(self.0.render_rgba(scene, (w, h), base, 1.0)?.pixels)
    }
}

/// A reusable headless renderer: one headless GPU device + renderer kept alive across
/// frames. Reuse one instance for multi-frame or multi-window capture — pipelines and atlases
/// stay cached. Works under whichever render backend the build selects.
///
/// Desktop only: the web has no headless device.
///
/// # Blocking
///
/// Each [`render_scene`](Self::render_scene) **blocks the calling thread for a full GPU
/// round-trip** (submit + map + readback) — potentially tens of milliseconds, and up to
/// [`GPU_POLL_TIMEOUT`] on a stalled driver before it returns `Err`. Do **not** call it from a
/// hot event handler or the live UI thread; use a background thread or a dedicated headless
/// process.
///
/// ```no_run
/// use pebbles_foundation::palette;
/// use pebbles_render::Scene;
/// use pebbles_shell::capture::OffscreenCapture;
///
/// let mut gpu = OffscreenCapture::new()?;
/// let scene = Scene::new(); // paint into it with a Ui, exactly as the shell does
/// let rgba = gpu.render_scene(&scene, 64, 32, palette::WHITE)?;
/// assert_eq!(rgba.len(), 64 * 32 * 4);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub struct OffscreenCapture {
    #[cfg(feature = "vello-hybrid")]
    inner: HybridCapture,
    #[cfg(all(feature = "wgpu", not(feature = "vello-hybrid")))]
    inner: WgpuCapture,
}

impl std::fmt::Debug for OffscreenCapture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OffscreenCapture").finish_non_exhaustive()
    }
}

impl OffscreenCapture {
    /// Acquire a headless GPU device and renderer. Blocks.
    pub fn new() -> Result<Self, CaptureError> {
        #[cfg(feature = "vello-hybrid")]
        let inner = HybridCapture::new()?;
        #[cfg(all(feature = "wgpu", not(feature = "vello-hybrid")))]
        let inner = WgpuCapture::new()?;
        Ok(Self { inner })
    }

    /// Render `scene` at `width`×`height` on an opaque `background` and return tightly packed,
    /// **unmultiplied RGBA8** (row-major, `width*height*4` bytes). A side of 0 or above
    /// [`MAX_CAPTURE_DIM`] is [`CaptureError::InvalidSize`].
    pub fn render_scene(
        &mut self,
        scene: &Scene,
        width: u32,
        height: u32,
        background: Color,
    ) -> Result<Vec<u8>, CaptureError> {
        if width == 0 || height == 0 || width > MAX_CAPTURE_DIM || height > MAX_CAPTURE_DIM {
            return Err(CaptureError::InvalidSize { width, height, max: MAX_CAPTURE_DIM });
        }
        // Force an opaque background on every backend.
        let [r, g, b, _] = background.components;
        self.inner.render(scene, width, height, Color::new([r, g, b, 1.0]))
    }
}

pub use pebbles_gpu::CaptureError;
#[cfg(feature = "wgpu")]
pub use pebbles_gpu::{OffscreenRenderer, RgbaImage};
#[allow(unused_qualifications)] // unqualified only when the wgpu re-export is compiled
const _: () = assert!(MAX_CAPTURE_DIM == pebbles_gpu::OffscreenRenderer::MAX_DIM);
