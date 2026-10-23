//! Platform policy: [`GpuInit`], env parsing, per-OS backends and adapter ranking.

use std::fmt;
use std::sync::Arc;

/// Adapter power preference used by the ranking in §3.1. DR-13: default `LowPower`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GpuPower {
    /// Prefer integrated (low-power) adapters.
    #[default]
    LowPower,
    /// Prefer discrete (high-performance) adapters.
    HighPerformance,
}

/// Where frames are rendered (08 §17.8). pebbles-gpu only carries the value; the shell acts on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderThreading {
    /// On the UI thread, inside `RedrawRequested`.
    Inline,
    /// On a dedicated `pebbles-render` thread (native only).
    Threaded,
}

/// The per-OS default (08 §17.8): `Inline` everywhere until P27 signs an OS off.
pub const fn default_threading(os: HostOs) -> RenderThreading {
    match os {
        HostOs::Web | HostOs::Other => RenderThreading::Inline,
        HostOs::Windows | HostOs::MacOs | HostOs::Linux => RenderThreading::Inline, // P27 sign-off per OS
    }
}

/// The platform family the policy is chosen for. Compile-time; never queried at runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostOs {
    /// Windows.
    Windows,
    /// macOS.
    MacOs,
    /// Linux.
    Linux,
    /// Web (wasm32).
    Web,
    /// Android, iOS, BSDs: out of scope — keep wgpu's defaults (compile-only).
    Other,
}

impl HostOs {
    /// The platform this build is running on.
    pub const fn current() -> Self {
        if cfg!(target_family = "wasm") {
            Self::Web
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Other
        }
    }
}

/// Everything the GPU layer needs to (re)create an instance, adapter, device and surface.
/// Built ONCE by the shell (or `headless()` for capture/render-check/tests) and cloned into every
/// recovery. Never mutated after construction except by the `with_*` builders.
#[derive(Clone)]
pub struct GpuInit {
    /// Display connection (winit's `OwnedDisplayHandle`). REQUIRED for the GL backend to
    /// present on Wayland and X11. `None` = headless.
    pub display: Option<Arc<dyn wgpu::wgt::WgpuHasDisplayHandle>>,
    /// The OS the policy below was resolved for (`HostOs::current()` outside tests).
    pub os: HostOs,
    /// Enabled backends, already env-resolved and intersected with the compiled set.
    pub backends: wgpu::Backends,
    /// Power preference driving adapter ranking.
    pub power: GpuPower,
    /// `PEBBLES_CPU_RENDER=1`: keep only `DeviceType::Cpu` adapters (WARP/lavapipe/llvmpipe).
    pub force_software: bool,
    /// `WGPU_ADAPTER_NAME`, lowercased: substring filter on `AdapterInfo::name`.
    pub adapter_name: Option<String>,
    /// Requested present mode; validated against the surface in §4.3. Default `AutoVsync`.
    pub present_mode: wgpu::PresentMode,
    /// `SurfaceConfiguration::desired_maximum_frame_latency`, clamped to 1..=3. Default 2.
    pub max_frame_latency: u32,
    /// DX12 shader compiler. DR-13: `Fxc` unless `WGPU_DX12_COMPILER` is set.
    pub dx12_compiler: wgpu::Dx12Compiler,
    /// `InstanceFlags::from_build_config().with_env()` (validation on in debug builds).
    pub flags: wgpu::InstanceFlags,
    /// `PEBBLES_GPU_INFO=1`: print the adapter and surface lines to stderr (§14.1).
    pub print_info: bool,
    /// Panic on any uncaptured wgpu error. Default `cfg!(debug_assertions)`;
    /// `PEBBLES_GPU_STRICT` overrides it.
    pub strict_validation: bool,
    /// `PEBBLES_GPU_TRACE` set (any value): log a backtrace with each uncaptured error.
    pub gpu_trace: bool,
    /// `PEBBLES_GPU_TIMING=1`: request `TIMESTAMP_QUERY` (when supported) and report
    /// `FrameStats::gpu_us` (M22, opt-in).
    pub gpu_timing: bool,
    /// Called from ANY thread when a device is lost. Must be cheap and non-blocking.
    pub on_device_lost: Option<Arc<dyn Fn() + Send + Sync>>,
    /// `PEBBLES_RENDER_THREAD`: where frames are rendered (08 §17.8).
    pub render_threading: RenderThreading,
}

impl fmt::Debug for GpuInit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GpuInit")
            .field("display", &self.display.is_some())
            .field("os", &self.os)
            .field("backends", &self.backends)
            .field("power", &self.power)
            .field("force_software", &self.force_software)
            .field("adapter_name", &self.adapter_name)
            .field("present_mode", &self.present_mode)
            .field("max_frame_latency", &self.max_frame_latency)
            .field("dx12_compiler", &self.dx12_compiler)
            .field("flags", &self.flags)
            .field("print_info", &self.print_info)
            .field("strict_validation", &self.strict_validation)
            .field("gpu_trace", &self.gpu_trace)
            .field("gpu_timing", &self.gpu_timing)
            .field("on_device_lost", &self.on_device_lost.is_some())
            .field("render_threading", &self.render_threading)
            .finish()
    }
}

impl GpuInit {
    /// `from_env_with(HostOs::current(), |k| std::env::var(k).ok())`. On wasm32
    /// `std::env::var` always fails, so every field takes its default.
    pub fn from_env() -> Self {
        Self::from_env_with(HostOs::current(), |k| std::env::var(k).ok())
    }

    /// The pure parser: read every env override through `get` (a `HashMap` in tests).
    pub fn from_env_with(os: HostOs, get: impl Fn(&str) -> Option<String>) -> Self {
        // 1. backends.
        let (backends, env_ignored) =
            resolve_backends(os, get("WGPU_BACKEND").as_deref(), wgpu::Instance::enabled_backend_features());
        if env_ignored && let Some(value) = get("WGPU_BACKEND") {
            warn(format_args!(
                "WGPU_BACKEND={value} selects no compiled backend; using the default {backends:?}"
            ));
        }
        // 2. power.
        let power = get("WGPU_POWER_PREF").and_then(|s| parse_power(&s)).unwrap_or_default();
        // 3. force_software.
        let force_software = get("PEBBLES_CPU_RENDER").is_some_and(|s| parse_flag(&s));
        // 4. adapter_name.
        let adapter_name = get("WGPU_ADAPTER_NAME").map(|s| s.to_lowercase()).filter(|s| !s.is_empty());
        // 5. present_mode.
        let present_mode = get("PEBBLES_PRESENT_MODE")
            .and_then(|s| parse_present_mode(&s))
            .unwrap_or(wgpu::PresentMode::AutoVsync);
        // 6. max_frame_latency.
        let max_frame_latency = get("PEBBLES_FRAME_LATENCY").and_then(|s| parse_latency(&s)).unwrap_or(2);
        // 7. dx12_compiler.
        let dx12_compiler = match get("WGPU_DX12_COMPILER") {
            Some(s) => match s.parse::<wgpu::Dx12Compiler>() {
                Ok(c) => c,
                Err(e) => {
                    warn(format_args!("unknown WGPU_DX12_COMPILER={s} ({e}); using Fxc"));
                    wgpu::Dx12Compiler::Fxc
                }
            },
            None => wgpu::Dx12Compiler::Fxc,
        };
        // 8. flags (reads the real environment; not injectable).
        let flags = wgpu::InstanceFlags::from_build_config().with_env();
        // 9. print_info / strict_validation / gpu_trace.
        let print_info = get("PEBBLES_GPU_INFO").is_some_and(|s| parse_flag(&s));
        let strict_validation = match get("PEBBLES_GPU_STRICT").map(|s| s.trim().to_ascii_lowercase()) {
            Some(s) if s == "1" || s == "true" => true,
            Some(s) if s == "0" || s == "false" => false,
            Some(s) => {
                warn(format_args!("unknown PEBBLES_GPU_STRICT={s}; using cfg!(debug_assertions)"));
                cfg!(debug_assertions)
            }
            None => cfg!(debug_assertions),
        };
        let gpu_trace = get("PEBBLES_GPU_TRACE").is_some();
        // 10. gpu_timing (M22).
        let gpu_timing = get("PEBBLES_GPU_TIMING").is_some_and(|s| parse_flag(&s));
        // 11. render_threading (08 §17.8): the web never threads.
        let render_threading = match (os, get("PEBBLES_RENDER_THREAD").as_deref().map(str::trim)) {
            (HostOs::Web, _) | (_, None) => default_threading(os),
            (_, Some("0")) => RenderThreading::Inline,
            (_, Some("1")) => RenderThreading::Threaded,
            (_, Some(s)) => {
                let fallback = default_threading(os);
                warn(format_args!("unknown PEBBLES_RENDER_THREAD={s}; using {fallback:?}"));
                fallback
            }
        };
        Self {
            display: None,
            os,
            backends,
            power,
            force_software,
            adapter_name,
            present_mode,
            max_frame_latency,
            dx12_compiler,
            flags,
            print_info,
            strict_validation,
            gpu_trace,
            gpu_timing,
            on_device_lost: None,
            render_threading,
        }
    }

    /// `from_env()` for offscreen use (no display; `on_device_lost` cleared).
    pub fn headless() -> Self {
        Self {
            display: None,
            on_device_lost: None,
            render_threading: RenderThreading::Inline,
            ..Self::from_env()
        }
    }

    /// Set the display connection (winit's `OwnedDisplayHandle`).
    #[must_use]
    pub fn with_display(self, d: Arc<dyn wgpu::wgt::WgpuHasDisplayHandle>) -> Self {
        Self { display: Some(d), ..self }
    }

    /// Set the device-lost waker (called from any thread).
    #[must_use]
    pub fn with_device_lost_waker(self, f: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self { on_device_lost: Some(f), ..self }
    }
}

fn warn(args: fmt::Arguments<'_>) {
    pebbles_foundation::log::logf(
        pebbles_foundation::log::Level::Warn,
        pebbles_foundation::log::Cat::Gpu,
        args,
    );
}

/// `WGPU_POWER_PREF` → `GpuPower` (`none` → LowPower). Unknown → None.
pub(crate) fn parse_power(s: &str) -> Option<GpuPower> {
    match s.trim().to_ascii_lowercase().as_str() {
        "low" | "none" => Some(GpuPower::LowPower),
        "high" => Some(GpuPower::HighPerformance),
        _ => None,
    }
}

/// `PEBBLES_PRESENT_MODE` → `wgpu::PresentMode`. Unknown → None.
pub(crate) fn parse_present_mode(s: &str) -> Option<wgpu::PresentMode> {
    Some(match s.trim().to_ascii_lowercase().as_str() {
        "vsync" => wgpu::PresentMode::AutoVsync,
        "novsync" => wgpu::PresentMode::AutoNoVsync,
        "fifo" => wgpu::PresentMode::Fifo,
        "fifo-relaxed" => wgpu::PresentMode::FifoRelaxed,
        "mailbox" => wgpu::PresentMode::Mailbox,
        "immediate" => wgpu::PresentMode::Immediate,
        _ => return None,
    })
}

/// `1`/`true` → on, anything else → off.
pub(crate) fn parse_flag(s: &str) -> bool {
    matches!(s.trim().to_ascii_lowercase().as_str(), "1" | "true")
}

/// `1`/`2`/`3` → that value; anything else → None.
pub(crate) fn parse_latency(s: &str) -> Option<u32> {
    s.trim().parse::<u32>().ok().filter(|n| (1..=3).contains(n))
}

/// Resolve the enabled backends: env (if set) else the per-OS default, intersected with what
/// this build compiled. Never returns an empty set unless nothing is compiled at all.
/// The `bool` is `true` when the env value was ignored.
pub(crate) fn resolve_backends(
    os: HostOs,
    env: Option<&str>,
    compiled: wgpu::Backends,
) -> (wgpu::Backends, bool) {
    let default = default_backends(os) & compiled;
    match env {
        None => (default, false),
        Some(list) => {
            let wanted = wgpu::Backends::from_comma_list(list) & compiled;
            if wanted.is_empty() { (default, true) } else { (wanted, false) }
        }
    }
}

/// The per-OS default backends (§2.4).
pub const fn default_backends(os: HostOs) -> wgpu::Backends {
    match os {
        HostOs::Windows => wgpu::Backends::DX12,
        HostOs::MacOs => wgpu::Backends::METAL,
        HostOs::Linux => wgpu::Backends::VULKAN.union(wgpu::Backends::GL),
        HostOs::Web => wgpu::Backends::BROWSER_WEBGPU.union(wgpu::Backends::GL),
        HostOs::Other => wgpu::Backends::all(),
    }
}

/// The per-OS backend ranking (best first) (§2.4).
#[cfg(not(target_family = "wasm"))]
pub const fn backend_rank(os: HostOs) -> &'static [wgpu::Backend] {
    use wgpu::Backend::{BrowserWebGpu, Dx12, Gl, Metal, Vulkan};
    match os {
        HostOs::Windows => &[Dx12, Vulkan, Gl],
        HostOs::MacOs => &[Metal],
        HostOs::Linux => &[Vulkan, Gl],
        HostOs::Web => &[BrowserWebGpu, Gl],
        HostOs::Other => &[Vulkan, Metal, Dx12, Gl],
    }
}

/// A ranked adapter candidate's identifying metadata.
#[cfg(not(target_family = "wasm"))]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AdapterCandidate {
    /// Adapter name.
    pub name: String,
    /// The backend that reported it.
    pub backend: wgpu::Backend,
    /// Hardware / integrated / virtual / cpu.
    pub device_type: wgpu::DeviceType,
}

#[cfg(not(target_family = "wasm"))]
impl AdapterCandidate {
    /// Build a candidate from wgpu's `AdapterInfo`.
    pub(crate) fn from_info(info: &wgpu::AdapterInfo) -> Self {
        Self { name: info.name.clone(), backend: info.backend, device_type: info.device_type }
    }
}

/// The result of ranking: an index order plus whether the name filter was ignored.
#[cfg(not(target_family = "wasm"))]
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Ranked {
    /// Indices into the candidate slice, best first. Empty = nothing acceptable.
    pub order: Vec<usize>,
    /// `WGPU_ADAPTER_NAME` was set but matched nothing (the filter was ignored).
    pub name_filter_ignored: bool,
}

/// Rank candidates by (1) hardware before software, (2) backend position, (3) device type.
#[cfg(not(target_family = "wasm"))]
pub(crate) fn rank_adapters(cands: &[AdapterCandidate], init: &GpuInit) -> Ranked {
    let rank = backend_rank(init.os);
    let mut order: Vec<usize> = (0..cands.len())
        .filter(|&i| !init.force_software || cands[i].device_type == wgpu::DeviceType::Cpu)
        .collect();
    let mut name_filter_ignored = false;
    if let Some(filter) = init.adapter_name.as_deref() {
        let hits: Vec<usize> =
            order.iter().copied().filter(|&i| cands[i].name.to_lowercase().contains(filter)).collect();
        if hits.is_empty() {
            name_filter_ignored = true;
        } else {
            order = hits;
        }
    }
    order.sort_by_key(|&i| {
        let c = &cands[i];
        let software = u8::from(c.device_type == wgpu::DeviceType::Cpu);
        let backend = rank.iter().position(|b| *b == c.backend).unwrap_or(usize::MAX);
        (software, backend, type_rank(c.device_type, init.power))
    });
    Ranked { order, name_filter_ignored }
}

#[cfg(not(target_family = "wasm"))]
fn type_rank(t: wgpu::DeviceType, power: GpuPower) -> u8 {
    use wgpu::DeviceType::{Cpu, DiscreteGpu, IntegratedGpu, Other, VirtualGpu};
    match (power, t) {
        (GpuPower::LowPower, IntegratedGpu) | (GpuPower::HighPerformance, DiscreteGpu) => 0,
        (GpuPower::LowPower, DiscreteGpu) | (GpuPower::HighPerformance, IntegratedGpu) => 1,
        (_, Other) => 2,
        (_, VirtualGpu) => 3,
        (_, Cpu) => 4,
    }
}

#[cfg(test)]
#[path = "init_tests.rs"]
mod tests;
