//! The adapter and surface diagnostic lines (08 §14.1, P09): pure formatters, plus the one
//! sanctioned stderr print (`PEBBLES_GPU_INFO=1`). On wasm32 the lines go to the browser
//! console instead (the web has no env; testers read them there, 08 §15).

use std::sync::atomic::{AtomicU64, Ordering};

use pebbles_foundation::log::{Cat, Level, logf};

use crate::init::GpuPower;
use crate::surface::OutputEncoding;

/// What the Device line reports.
pub(crate) struct GpuSummary<'a> {
    pub name: &'a str,
    pub backend: wgpu::Backend,
    pub device_type: wgpu::DeviceType,
    pub driver: &'a str,
    pub driver_info: &'a str,
    pub max_texture_dim: u32,
    pub power: GpuPower,
}

/// `GPU: {name} | backend={backend} type={type:?} | driver="{driver} {info}" | max_tex={n} | power={power:?}[ | software]`
pub(crate) fn adapter_line(s: &GpuSummary<'_>) -> String {
    let soft = if s.device_type == wgpu::DeviceType::Cpu { " | software" } else { "" };
    format!(
        "GPU: {} | backend={} type={:?} | driver=\"{} {}\" | max_tex={} | power={:?}{soft}",
        s.name,
        s.backend.to_str(),
        s.device_type,
        s.driver,
        s.driver_info,
        s.max_texture_dim,
        s.power
    )
}

/// What the Surface line reports.
pub(crate) struct SurfaceSummary {
    pub format: wgpu::TextureFormat,
    pub view_format: wgpu::TextureFormat,
    pub encoding: OutputEncoding,
    pub present: wgpu::PresentMode,
    pub latency: u32,
    pub alpha: wgpu::CompositeAlphaMode,
    pub width: u32,
    pub height: u32,
}

/// `surface: format={f:?} view={v:?} encoding={e:?} present={p:?} latency={n} alpha={a:?} size={w}x{h}`
pub(crate) fn surface_line(s: &SurfaceSummary) -> String {
    format!(
        "surface: format={:?} view={:?} encoding={:?} present={:?} latency={} alpha={:?} size={}x{}",
        s.format, s.view_format, s.encoding, s.present, s.latency, s.alpha, s.width, s.height
    )
}

/// `pebbles: {line}` on stderr (native) or the browser console (wasm32).
#[allow(clippy::print_stderr)] // PEBBLES_GPU_INFO, 08 §14.1
pub(crate) fn stderr_line(line: &str) {
    #[cfg(not(target_family = "wasm"))]
    eprintln!("pebbles: {line}");
    #[cfg(target_family = "wasm")]
    web_sys::console::info_1(&format!("pebbles: {line}").into());
}

/// Log once per face that a malformed font panicked inside the font stack (its glyphs are skipped).
pub(crate) fn warn_once_font_panic(blob_id: u64) {
    thread_local! {
        static SEEN: std::cell::RefCell<std::collections::HashSet<u64>> = std::cell::RefCell::default();
    }
    if SEEN.with(|s| s.borrow_mut().insert(blob_id)) {
        pebbles_foundation::log::warn(
            Cat::Gpu,
            format!(
                "font {blob_id:#x}: malformed glyph data panicked in the font parser; its glyphs are skipped"
            ),
        );
    }
}

// The `log` facade is process-global, so its bridge and counter are statics: the one
// sanctioned exception to 06 §5.3.
static BACKEND_ERRORS: AtomicU64 = AtomicU64::new(0);

struct GpuLogBridge;
static BRIDGE: GpuLogBridge = GpuLogBridge;

fn is_gpu_target(t: &str) -> bool {
    t.starts_with("wgpu") || t.starts_with("naga")
}

impl log::Log for GpuLogBridge {
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        m.level() <= log::Level::Warn && is_gpu_target(m.target())
    }
    fn log(&self, r: &log::Record<'_>) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let level = if r.level() == log::Level::Error { Level::Error } else { Level::Warn };
        if r.level() == log::Level::Error {
            BACKEND_ERRORS.fetch_add(1, Ordering::Relaxed);
        }
        logf(level, Cat::Gpu, format_args!("[{}] {}", r.target(), r.args()));
    }
    fn flush(&self) {}
}

/// Install the bridge unless the application already installed a `log` logger. Idempotent.
pub fn install_log_bridge() {
    if log::set_logger(&BRIDGE).is_ok() {
        log::set_max_level(log::LevelFilter::Warn);
    }
}

/// Error-level wgpu/naga `log` records seen so far (backend validation layers, hal errors).
pub fn backend_error_count() -> u64 {
    BACKEND_ERRORS.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adapter_line_format() {
        let s = GpuSummary {
            name: "llvmpipe (LLVM 17.0.6, 256 bits)",
            backend: wgpu::Backend::Vulkan,
            device_type: wgpu::DeviceType::Cpu,
            driver: "llvmpipe",
            driver_info: "Mesa 24.0",
            max_texture_dim: 16384,
            power: GpuPower::LowPower,
        };
        assert_eq!(
            adapter_line(&s),
            "GPU: llvmpipe (LLVM 17.0.6, 256 bits) | backend=vulkan type=Cpu | driver=\"llvmpipe Mesa 24.0\" | max_tex=16384 | power=LowPower | software"
        );
    }

    #[test]
    fn surface_line_format() {
        let s = SurfaceSummary {
            format: wgpu::TextureFormat::Bgra8Unorm,
            view_format: wgpu::TextureFormat::Bgra8Unorm,
            encoding: OutputEncoding::Gamma,
            present: wgpu::PresentMode::AutoVsync,
            latency: 2,
            alpha: wgpu::CompositeAlphaMode::Opaque,
            width: 800,
            height: 600,
        };
        assert_eq!(
            surface_line(&s),
            "surface: format=Bgra8Unorm view=Bgra8Unorm encoding=Gamma present=AutoVsync latency=2 alpha=Opaque size=800x600"
        );
    }

    #[test]
    fn bridge_forwards_and_counts_errors() {
        install_log_bridge();
        install_log_bridge(); // idempotent
        let before = backend_error_count();
        log::error!(target: "wgpu_hal::test", "x");
        log::error!(target: "my_app", "not a GPU target"); // never forwarded or counted
        assert_eq!(backend_error_count(), before + 1);
        let snap = pebbles_foundation::log::snapshot();
        assert!(snap.iter().any(|r| r.msg == "[wgpu_hal::test] x"), "no forwarded record");
        assert!(!snap.iter().any(|r| r.msg.contains("not a GPU target")));
    }
}
