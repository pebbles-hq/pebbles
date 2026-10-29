//! The wgpu instance, adapter and device pool.

use std::sync::Arc;

use pebbles_foundation::log::{Cat, Level, logf};

use super::error::GpuError;
pub use super::health::DeviceHealth;
use super::health::install_device_hooks;
use super::init::GpuInit;
#[cfg(target_family = "wasm")]
use super::init::GpuPower;
#[cfg(not(target_family = "wasm"))]
use super::init::{AdapterCandidate, first_ok, rank_adapters};

/// Index of a device in its [`GpuContext`]. Invalid after the context is dropped/rebuilt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DeviceId(usize);

impl DeviceId {
    /// The underlying `usize` index.
    pub fn index(self) -> usize {
        self.0
    }
}

/// What the device can do, captured once at creation.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct GpuCaps {
    /// The adapter that backed this device.
    pub info: wgpu::AdapterInfo,
    /// The limits the device was CREATED with (not the adapter's maximum).
    pub limits: wgpu::Limits,
    /// The adapter's downlevel capabilities.
    pub downlevel: wgpu::DownlevelCapabilities,
    /// 4 if `sample_count_supported(4)` for Bgra8Unorm AND Rgba8Unorm, else 1.
    pub msaa_samples: u32,
    /// `DownlevelFlags::SURFACE_VIEW_FORMATS` (false on GL/WebGL).
    pub surface_view_formats: bool,
    /// `info.backend == wgpu::Backend::Gl` — enables the downlevel rules of 08 §11.
    pub is_gl: bool,
}

/// One adapter + device + queue.
pub struct GpuDevice {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    caps: GpuCaps,
    health: Arc<DeviceHealth>,
}

impl GpuDevice {
    /// The adapter.
    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    /// The device.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// The queue.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// What this device can do (captured at creation).
    pub fn caps(&self) -> &GpuCaps {
        &self.caps
    }

    /// Loss and error state, written by the device hooks.
    pub fn health(&self) -> &Arc<DeviceHealth> {
        &self.health
    }
}

/// The wgpu instance + device pool. Rebuilt wholesale on device loss (C18).
pub struct GpuContext {
    instance: wgpu::Instance,
    init: GpuInit,
    devices: Vec<GpuDevice>,
    /// `PEBBLES_SYNTH_GPU_DEAD` (soak/test tool): this context finds no adapter.
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    synth_dead: bool,
}

/// Soak/test tool (08 §6.7, P19): with `PEBBLES_SYNTH_GPU_DEAD=1`, every context after the
/// process's first finds no adapter — a GPU that vanished after startup (an unplugged eGPU, a
/// driver that never comes back). Combine with `PEBBLES_SYNTH_GPU_LOSS=N` to lose the first
/// device; every rebuild then fails like a real missing driver, and the reset backoff runs.
/// Native only; unset in normal runs.
fn synth_gpu_dead() -> bool {
    #[cfg(not(target_family = "wasm"))]
    {
        use std::sync::atomic::{AtomicU32, Ordering};
        static CONTEXTS: AtomicU32 = AtomicU32::new(0);
        let n = CONTEXTS.fetch_add(1, Ordering::Relaxed);
        n > 0 && std::env::var("PEBBLES_SYNTH_GPU_DEAD").is_ok_and(|v| v == "1")
    }
    #[cfg(target_family = "wasm")]
    false
}

/// Build the [`wgpu::InstanceDescriptor`] from [`GpuInit`] (never the env-blind default instance).
pub(crate) fn instance_descriptor(init: &GpuInit) -> wgpu::InstanceDescriptor {
    let mut desc = match &init.display {
        // `Box<Arc<dyn WgpuHasDisplayHandle>>` coerces to `Box<dyn WgpuHasDisplayHandle>`.
        Some(d) => wgpu::InstanceDescriptor::new_with_display_handle(Box::new(Arc::clone(d))),
        None => wgpu::InstanceDescriptor::new_without_display_handle(),
    };
    desc.backends = init.backends;
    desc.flags = init.flags;
    desc.backend_options.dx12.shader_compiler = init.dx12_compiler.clone();
    // GL minor version / DX12 presentation system / noop honour their WGPU_* variables.
    desc.backend_options = desc.backend_options.with_env();
    // 11 §2.6 rule 5: memory pressure surfaces as a recoverable OOM (D3D12; optionally Vulkan).
    // Never `InstanceDescriptor::with_env()`: it would re-read WGPU_BACKEND and reset this.
    desc.memory_budget_thresholds =
        wgpu::MemoryBudgetThresholds { for_resource_creation: Some(95), for_device_loss: None };
    desc
}

/// 4x when the format supports it, else 1x. Never 2x/8x.
pub(crate) const fn pick_msaa(supports_4x: bool) -> u32 {
    if supports_4x { 4 } else { 1 }
}

impl GpuContext {
    /// Create a context from `init` (env-aware instance).
    pub async fn new(init: &GpuInit) -> Self {
        let instance = wgpu::util::new_instance_with_webgpu_detection(instance_descriptor(init)).await;
        logf(
            Level::Info,
            Cat::Gpu,
            format_args!(
                "GPU instance: compiled={:?} enabled={:?} display={}",
                wgpu::Instance::enabled_backend_features(),
                init.backends,
                if init.display.is_some() { "yes" } else { "no" }
            ),
        );
        Self { instance, init: init.clone(), devices: Vec::new(), synth_dead: synth_gpu_dead() }
    }

    /// Create + configure a surface for `window` (on its compatible device).
    pub async fn create_surface<'window>(
        &mut self,
        window: impl Into<wgpu::SurfaceTarget<'window>>,
        width: u32,
        height: u32,
    ) -> Result<super::surface::RenderSurface<'window>, GpuError> {
        let surface = match self.instance.create_surface(window) {
            Ok(s) => s,
            Err(e) => {
                // 08 §14.3: no backend could even make a surface because none has an adapter —
                // that is a missing driver (NoAdapter guidance), not a window-system refusal.
                #[cfg(not(target_family = "wasm"))]
                if self.devices.is_empty()
                    && self.instance.enumerate_adapters(self.init.backends).await.is_empty()
                {
                    return Err(GpuError::NoAdapter {
                        backends: self.init.backends,
                        software_only: self.init.force_software,
                        enumerated: 0,
                        source: None,
                    });
                }
                return Err(GpuError::from(e));
            }
        };
        self.adopt_surface(surface, (width, height)).await
    }

    /// The instance this context's surfaces are created with (a cheap clone, 08 §17.2): the UI
    /// thread creates surfaces with it and hands them to [`adopt_surface`](Self::adopt_surface).
    pub fn instance(&self) -> wgpu::Instance {
        self.instance.clone()
    }

    /// Finish a surface created from [`instance`](Self::instance): pick its device (§3.7), format,
    /// alpha and present mode (§4.1–§4.3) and configure it at `size` (§4.4).
    pub async fn adopt_surface<'window>(
        &mut self,
        surface: wgpu::Surface<'window>,
        size: (u32, u32),
    ) -> Result<super::surface::RenderSurface<'window>, GpuError> {
        super::surface::RenderSurface::configure_new(self, surface, size.0, size.1).await
    }

    /// Soak tooling (`PEBBLES_SYNTH_GPU_LOSS`, 08 §6.9): destroys every device so the real
    /// lost-callback path runs.
    pub fn simulate_device_loss(&self) {
        for d in &self.devices {
            d.device.destroy();
            let _ = d.device.poll(wgpu::PollType::Poll);
            // Vulkan fires the lost callback inside `destroy`; Metal and DX12 defer it past the
            // poll (or never fire it once the device is dropped by the reset), so the reset ran
            // without its `reason=Destroyed` line. The destroy is certain: record it now (the
            // callback, if it fires later, records the same reason).
            if !d.health.is_lost() {
                d.health.mark_lost(wgpu::DeviceLostReason::Destroyed);
            }
        }
    }

    /// Reuse a compatible device, or request a new one through the §3.1 ranking.
    pub async fn request_device(
        &mut self,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<DeviceId, GpuError> {
        // Reuse: an existing device whose adapter can present to this surface.
        if let Some(index) = compatible_surface.and_then(|surface| {
            self.devices.iter().position(|device| device.adapter.is_surface_supported(surface))
        }) {
            return Ok(DeviceId(index));
        }
        // Headless (`None`) reuses device 0.
        if compatible_surface.is_none() && !self.devices.is_empty() {
            return Ok(DeviceId(0));
        }

        #[cfg(target_family = "wasm")]
        {
            // WebGL2 requires a compatible surface; never request before one exists.
            let adapter = self
                .instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: match self.init.power {
                        GpuPower::LowPower => wgpu::PowerPreference::LowPower,
                        GpuPower::HighPerformance => wgpu::PowerPreference::HighPerformance,
                    },
                    force_fallback_adapter: false,
                    compatible_surface,
                })
                .await
                .map_err(|source| GpuError::NoAdapter {
                    backends: self.init.backends,
                    software_only: false,
                    enumerated: 0,
                    source: Some(source),
                })?;
            self.finish_device(&adapter).await
        }

        #[cfg(not(target_family = "wasm"))]
        {
            if self.synth_dead {
                logf(Level::Warn, Cat::Gpu, format_args!("PEBBLES_SYNTH_GPU_DEAD: no adapter (synthetic)"));
                return Err(GpuError::NoAdapter {
                    backends: self.init.backends,
                    software_only: self.init.force_software,
                    enumerated: 0,
                    source: None,
                });
            }
            let all = self.instance.enumerate_adapters(self.init.backends).await;
            let enumerated = all.len();
            let adapters: Vec<wgpu::Adapter> = all
                .into_iter()
                .filter(|a| compatible_surface.is_none_or(|s| a.is_surface_supported(s)))
                .collect();
            let candidates: Vec<AdapterCandidate> =
                adapters.iter().map(|a| AdapterCandidate::from_info(&a.get_info())).collect();
            let ranked = rank_adapters(&candidates, &self.init);
            if ranked.name_filter_ignored
                && let Some(filter) = self.init.adapter_name.as_deref()
            {
                logf(
                    Level::Warn,
                    Cat::Gpu,
                    format_args!("WGPU_ADAPTER_NAME=\"{filter}\" matched no adapter; ignoring it"),
                );
            }
            let (backends, software_only) = (self.init.backends, self.init.force_software);
            // 08 §3.1: a ranked adapter whose device request fails (a broken or mismatched
            // driver: an NVIDIA kernel/userspace mismatch, a stale ICD, a hybrid laptop's
            // powered-down dGPU) falls through to the next one instead of failing start-up.
            first_ok(
                self,
                &ranked.order,
                async |ctx: &mut Self, i: usize| ctx.finish_device(&adapters[i]).await,
                |i, e| {
                    logf(
                        Level::Warn,
                        Cat::Gpu,
                        format_args!(
                            "GPU adapter \"{}\" failed ({e}); trying the next one",
                            candidates[i].name
                        ),
                    );
                },
                || GpuError::NoAdapter { backends, software_only, enumerated, source: None },
            )
            .await
        }
    }

    /// Request the device for `adapter` with the portable limits profile, probe caps,
    /// log the §14.1 line and push the [`GpuDevice`].
    async fn finish_device(&mut self, adapter: &wgpu::Adapter) -> Result<DeviceId, GpuError> {
        let info = adapter.get_info();
        let limits = wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
        // M22: timestamps only when asked for and supported (never by default).
        let timing = self.init.gpu_timing && adapter.features().contains(wgpu::Features::TIMESTAMP_QUERY);
        let required_features =
            if timing { wgpu::Features::TIMESTAMP_QUERY } else { wgpu::Features::empty() };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("pebbles.gpu.device"),
                required_features,
                required_limits: limits,
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                ..Default::default()
            })
            .await
            .map_err(|source| GpuError::RequestDevice { adapter: info.name.clone(), source })?;

        let downlevel = adapter.get_downlevel_capabilities();
        let surface_view_formats = downlevel.flags.contains(wgpu::DownlevelFlags::SURFACE_VIEW_FORMATS);
        let is_gl = info.backend == wgpu::Backend::Gl;
        // The common-format summary; renderers negotiate per format through `MsaaCache` (R12).
        let count = |f| super::frame_targets::sample_count_for(adapter, f, None);
        let msaa_samples = pick_msaa(
            count(wgpu::TextureFormat::Bgra8Unorm) == 4 && count(wgpu::TextureFormat::Rgba8Unorm) == 4,
        );
        let caps = GpuCaps {
            info: info.clone(),
            limits: device.limits(),
            downlevel,
            msaa_samples,
            surface_view_formats,
            is_gl,
        };

        let software = info.device_type == wgpu::DeviceType::Cpu;
        let line = super::diag::adapter_line(&super::diag::GpuSummary {
            name: &info.name,
            backend: info.backend,
            device_type: info.device_type,
            driver: &info.driver,
            driver_info: &info.driver_info,
            max_texture_dim: caps.limits.max_texture_dimension_2d,
            power: self.init.power,
        });
        logf(Level::Info, Cat::Gpu, format_args!("{line}"));
        // P09: PEBBLES_GPU_INFO=1 (native) — the web always writes it to the console.
        if self.init.print_info || cfg!(target_family = "wasm") {
            super::diag::stderr_line(&line);
        }
        if software && !self.init.force_software {
            logf(
                Level::Warn,
                Cat::Gpu,
                format_args!(
                    "no hardware GPU adapter; using software adapter \"{}\" — expect high CPU use",
                    info.name
                ),
            );
        }

        let health = Arc::new(DeviceHealth::default());
        install_device_hooks(&device, &health, &self.init);
        self.devices.push(GpuDevice { adapter: adapter.clone(), device, queue, caps, health });
        Ok(DeviceId(self.devices.len() - 1))
    }

    /// The device at `id`, if any.
    pub fn device(&self, id: DeviceId) -> Option<&GpuDevice> {
        self.devices.get(id.0)
    }

    /// Iterate `(id, device)` pairs.
    pub fn devices(&self) -> impl Iterator<Item = (DeviceId, &GpuDevice)> {
        self.devices.iter().enumerate().map(|(i, d)| (DeviceId(i), d))
    }

    /// Whether any device is lost or hit a fatal error (the shell rebuilds the context).
    pub fn any_device_poisoned(&self) -> bool {
        self.devices.iter().any(|d| d.health.is_poisoned())
    }

    /// Every lost device with its reason and driver message (for the frame loop's log line).
    pub fn lost_devices(&self) -> Vec<(DeviceId, wgpu::DeviceLostReason, String)> {
        self.devices
            .iter()
            .enumerate()
            .filter_map(|(i, d)| {
                let reason = d.health.lost_reason()?;
                Some((DeviceId(i), reason, d.health.lost_message().unwrap_or_default()))
            })
            .collect()
    }

    /// The [`GpuInit`] this context was created with.
    pub fn init(&self) -> &GpuInit {
        &self.init
    }
}

#[cfg(test)]
#[path = "context_tests.rs"]
pub(crate) mod tests;
