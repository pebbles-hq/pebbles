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
        Ok(WgpuCapture(OffscreenRenderer::with_init(pebbles_gpu::GpuInit::headless())?))
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

/// Render `scene` at `size` (device px) scaled by `scale` on white and compare it with the app's
/// golden `tests/goldens/<name>.png` (under the crate being tested). Pebbles renders the same
/// pixels on every backend (DR-21), so one golden serves every OS: the comparison uses the
/// cross-backend limits — a pixel may differ by 1 (rounding), at most 0.10 % of pixels by
/// more, and the mean absolute error stays ≤ 0.05.
///
/// On a mismatch it writes `<name>.actual.png` and `<name>.diff.png` next to the golden and
/// panics with the numbers. `PEBBLES_UPDATE_GOLDENS=1` writes the golden instead. Without a GPU
/// adapter the check is skipped with a message, unless `PEBBLES_REQUIRE_GPU=1`.
///
/// Build `scene` with `pebbles_testing::Harness::draw()` from a harness on the bundled fonts
/// (`TextEnv::new()`), never system fonts, so the golden is the same on every machine.
///
/// # Panics
///
/// On a mismatch, an unreadable golden, or a missing golden outside update mode.
#[cfg(not(target_family = "wasm"))]
pub fn assert_scene_matches_golden(scene: &Scene, size: (u32, u32), scale: f64, name: &str) {
    let mut gpu = match OffscreenRenderer::new() {
        Ok(gpu) => gpu,
        Err(e) if std::env::var("PEBBLES_REQUIRE_GPU").is_ok_and(|v| v == "1") => {
            panic!("golden `{name}`: PEBBLES_REQUIRE_GPU=1 but no GPU adapter: {e}")
        }
        Err(e) => {
            eprintln!("golden `{name}`: skipped, no GPU adapter ({e})");
            return;
        }
    };
    let actual = gpu
        .render_rgba(scene, size, pebbles_foundation::palette::WHITE, scale)
        .expect("render the golden scene");
    let dir = std::env::var_os("CARGO_MANIFEST_DIR")
        .map_or_else(|| std::path::PathBuf::from("."), std::path::PathBuf::from)
        .join("tests/goldens");
    let update = std::env::var("PEBBLES_UPDATE_GOLDENS").is_ok_and(|v| v == "1");
    if let Err(e) = golden::check(&dir, name, size, &actual.pixels, update) {
        panic!("golden `{name}`: {e}");
    }
}

/// The golden comparison (05 §6.10 metric, cross-backend limits).
#[cfg(not(target_family = "wasm"))]
mod golden {
    use std::path::Path;

    /// 05 §8.1 `CrossBackend`.
    const OVER_THRESHOLD: u8 = 1;
    const MAX_PCT_OVER: f64 = 0.10;
    const MAX_INTERIOR: u8 = 1;
    const MAX_MAE: f64 = 0.05;

    pub(super) fn check(
        dir: &Path,
        name: &str,
        size: (u32, u32),
        actual: &[u8],
        update: bool,
    ) -> Result<(), String> {
        let golden = dir.join(format!("{name}.png"));
        if update {
            std::fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
            return write_png(&golden, size, actual);
        }
        let (gsize, expected) = read_png(&golden).map_err(|e| {
            format!("{e} — run with PEBBLES_UPDATE_GOLDENS=1 to record it ({})", golden.display())
        })?;
        if gsize != size {
            return Err(format!(
                "size {}×{} differs from the golden's {}×{}",
                size.0, size.1, gsize.0, gsize.1
            ));
        }
        let m = compare(size, &expected, actual);
        let pass = m.max_interior <= MAX_INTERIOR && m.pct_over <= MAX_PCT_OVER && m.mae <= MAX_MAE;
        if pass {
            return Ok(());
        }
        let diff: Vec<u8> = expected
            .as_chunks::<4>()
            .0
            .iter()
            .zip(actual.as_chunks::<4>().0.iter())
            .flat_map(|(a, b)| {
                let d = (0..4).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0);
                if d > OVER_THRESHOLD {
                    [255, 0, 255, 255]
                } else {
                    [a[0] / 4 + 191, a[1] / 4 + 191, a[2] / 4 + 191, 255]
                }
            })
            .collect();
        let _ = write_png(&dir.join(format!("{name}.actual.png")), size, actual);
        let _ = write_png(&dir.join(format!("{name}.diff.png")), size, &diff);
        Err(format!(
            "does not match {}: max_interior_delta {} (≤ {MAX_INTERIOR}), pct_over {:.4} % (≤ {MAX_PCT_OVER}), \
             mae {:.4} (≤ {MAX_MAE}), max_delta {}; wrote {name}.actual.png and {name}.diff.png",
            golden.display(),
            m.max_interior,
            m.pct_over,
            m.mae,
            m.max_delta
        ))
    }

    struct Metrics {
        max_delta: u8,
        max_interior: u8,
        pct_over: f64,
        mae: f64,
    }

    fn luma(p: [u8; 4]) -> i64 {
        i64::from((2126 * u32::from(p[0]) + 7152 * u32::from(p[1]) + 722 * u32::from(p[2])) / 10000)
    }

    /// Sobel on luma (clamp-to-edge), `|Gx| + |Gy| > 64`, dilated by 1 px.
    fn edges(size: (u32, u32), rgba: &[u8]) -> Vec<bool> {
        let (w, h) = (i64::from(size.0), i64::from(size.1));
        let l: Vec<i64> = rgba.as_chunks::<4>().0.iter().map(|p| luma(*p)).collect();
        let at = |x: i64, y: i64| l[(y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) as usize];
        let raw: Vec<bool> = (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| {
                let gx = at(x + 1, y - 1) + 2 * at(x + 1, y) + at(x + 1, y + 1)
                    - at(x - 1, y - 1)
                    - 2 * at(x - 1, y)
                    - at(x - 1, y + 1);
                let gy = at(x - 1, y + 1) + 2 * at(x, y + 1) + at(x + 1, y + 1)
                    - at(x - 1, y - 1)
                    - 2 * at(x, y - 1)
                    - at(x + 1, y - 1);
                gx.abs() + gy.abs() > 64
            })
            .collect();
        (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| {
                (-1..=1).any(|dy| {
                    (-1..=1).any(|dx| {
                        let (nx, ny) = (x + dx, y + dy);
                        nx >= 0 && ny >= 0 && nx < w && ny < h && raw[(ny * w + nx) as usize]
                    })
                })
            })
            .collect()
    }

    fn compare(size: (u32, u32), expected: &[u8], actual: &[u8]) -> Metrics {
        let mask: Vec<bool> =
            edges(size, expected).into_iter().zip(edges(size, actual)).map(|(a, b)| a || b).collect();
        let (mut max_delta, mut max_interior, mut over, mut sum) = (0u8, 0u8, 0u64, 0u64);
        for (i, (a, b)) in
            expected.as_chunks::<4>().0.iter().zip(actual.as_chunks::<4>().0.iter()).enumerate()
        {
            let d = (0..4).map(|c| a[c].abs_diff(b[c])).max().unwrap_or(0);
            sum += (0..4).map(|c| u64::from(a[c].abs_diff(b[c]))).sum::<u64>();
            max_delta = max_delta.max(d);
            if !mask[i] {
                max_interior = max_interior.max(d);
            }
            over += u64::from(d > OVER_THRESHOLD);
        }
        let pixels = (mask.len() as f64).max(1.0);
        Metrics {
            max_delta,
            max_interior,
            pct_over: 100.0 * over as f64 / pixels,
            mae: sum as f64 / (4.0 * pixels),
        }
    }

    pub(super) fn write_png(path: &Path, size: (u32, u32), rgba: &[u8]) -> Result<(), String> {
        let file = std::fs::File::create(path).map_err(|e| format!("create {}: {e}", path.display()))?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), size.0, size.1);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .and_then(|mut w| w.write_image_data(rgba))
            .map_err(|e| format!("write {}: {e}", path.display()))
    }

    fn read_png(path: &Path) -> Result<((u32, u32), Vec<u8>), String> {
        let file = std::fs::File::open(path).map_err(|e| format!("no golden at {}: {e}", path.display()))?;
        let mut reader = png::Decoder::new(std::io::BufReader::new(file))
            .read_info()
            .map_err(|e| format!("read {}: {e}", path.display()))?;
        let mut buf = vec![0; reader.output_buffer_size().unwrap_or(0)];
        let info = reader.next_frame(&mut buf).map_err(|e| format!("decode {}: {e}", path.display()))?;
        if (info.color_type, info.bit_depth) != (png::ColorType::Rgba, png::BitDepth::Eight) {
            return Err(format!("{} is not 8-bit RGBA", path.display()));
        }
        buf.truncate(info.buffer_size());
        Ok(((info.width, info.height), buf))
    }
}

pub use pebbles_gpu::CaptureError;
pub use pebbles_gpu::{OffscreenRenderer, RgbaImage};
const _: () = assert!(MAX_CAPTURE_DIM == OffscreenRenderer::MAX_DIM);

#[cfg(all(test, not(target_family = "wasm")))]
mod golden_tests {
    use super::golden::{check, write_png};

    /// A 64×48 flat grey image with a dark square: interior pixels away from every edge.
    fn image() -> Vec<u8> {
        let mut px = [200u8, 200, 200, 255].repeat(64 * 48);
        for y in 10..30 {
            for x in 10..30 {
                px[(y * 64 + x) * 4..][..3].copy_from_slice(&[40, 40, 40]);
            }
        }
        px
    }

    fn dir(test: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("pebbles-golden-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn one_pixel_change_fails() {
        let d = dir("one-pixel");
        check(&d, "g", (64, 48), &image(), true).expect("write golden");
        // One pixel turned transparent black. (A lone changed pixel is its own Sobel edge, so it is
        // judged by the mean error: 855 / (4 × 3072) ≈ 0.07 > 0.05.)
        let mut changed = image();
        changed[(40 * 64 + 50) * 4..][..4].copy_from_slice(&[0, 0, 0, 0]);
        let err = check(&d, "g", (64, 48), &changed, false).expect_err("a changed pixel fails");
        assert!(err.contains("max_delta 255"), "{err}");
        assert!(d.join("g.actual.png").exists() && d.join("g.diff.png").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn cross_backend_tolerance_passes() {
        let d = dir("tolerance");
        write_png(&d.join("g.png"), (64, 48), &image()).unwrap_or_else(|_| {
            std::fs::create_dir_all(&d).unwrap();
            write_png(&d.join("g.png"), (64, 48), &image()).unwrap();
        });
        // ±1 LSB in 0.05 % of the pixels (2 of 3072): rounding differences between backends.
        let mut nudged = image();
        nudged[(5 * 64 + 5) * 4] += 1;
        nudged[(40 * 64 + 60) * 4 + 1] -= 1;
        check(&d, "g", (64, 48), &nudged, false).expect("±1 LSB passes");
        let _ = std::fs::remove_dir_all(&d);
    }
}
