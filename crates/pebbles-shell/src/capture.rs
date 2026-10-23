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

use crate::gpu;

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

/// The inner renderer: pebbles-gpu's public headless renderer (C15).
struct WgpuCapture(OffscreenRenderer);

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
/// stay cached.
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
        Ok(Self { inner: WgpuCapture::new()? })
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
        // Force an opaque background.
        let [r, g, b, _] = background.components;
        self.inner.render(scene, width, height, Color::new([r, g, b, 1.0]))
    }
}

pub use pebbles_gpu::CaptureError;
pub use pebbles_gpu::{OffscreenRenderer, RgbaImage};
const _: () = assert!(MAX_CAPTURE_DIM == OffscreenRenderer::MAX_DIM);
