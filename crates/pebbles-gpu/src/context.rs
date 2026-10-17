//! The wgpu instance, adapter and device pool.

use std::collections::HashSet;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64};
use std::sync::{Arc, Mutex};

use pebbles_foundation::log::{Cat, Level, logf};

use super::error::GpuError;
use super::init::GpuInit;
#[cfg(target_family = "wasm")]
use super::init::GpuPower;
#[cfg(not(target_family = "wasm"))]
use super::init::{AdapterCandidate, rank_adapters};

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

/// Per-device health, written by wgpu callbacks (any thread), read by the frame loop (08 §6.2).
#[derive(Debug, Default)]
pub struct DeviceHealth {
    lost: AtomicBool,
    /// 0 = not lost, 1 = Unknown (real loss), 2 = Destroyed.
    lost_reason: AtomicU8,
    fatal_errors: AtomicU64,
    validation_errors: AtomicU64,
    /// Hashes of validation messages already logged (dedup).
    seen: Mutex<HashSet<u64>>,
    /// The driver's message from the lost callback; the frame loop logs it.
    lost_message: Mutex<Option<String>>,
}

impl DeviceHealth {
    /// Lost, or hit an out-of-memory / internal error: the device must be rebuilt.
    pub fn is_poisoned(&self) -> bool {
        self.lost.load(Relaxed) || self.fatal_errors.load(Relaxed) > 0
    }

    /// The lost callback fired.
    pub fn is_lost(&self) -> bool {
        self.lost.load(Relaxed)
    }

    /// The driver's message from the lost callback.
    pub fn lost_message(&self) -> Option<String> {
        self.lost_message.lock().ok().and_then(|m| m.clone())
    }

    /// Why the device was lost, if it was.
    pub fn lost_reason(&self) -> Option<wgpu::DeviceLostReason> {
        match self.lost_reason.load(Relaxed) {
            1 => Some(wgpu::DeviceLostReason::Unknown),
            2 => Some(wgpu::DeviceLostReason::Destroyed),
            _ => None,
        }
    }

    /// Out-of-memory and internal errors seen.
    pub fn fatal_errors(&self) -> u64 {
        self.fatal_errors.load(Relaxed)
    }

    /// Validation errors seen.
    pub fn validation_errors(&self) -> u64 {
        self.validation_errors.load(Relaxed)
    }

    pub(crate) fn mark_lost(&self, reason: wgpu::DeviceLostReason) {
        let code = match reason {
            wgpu::DeviceLostReason::Unknown => 1,
            wgpu::DeviceLostReason::Destroyed => 2,
        };
        self.lost_reason.store(code, Relaxed);
        self.lost.store(true, Relaxed);
    }
}

/// Install the lost callback and the uncaptured-error handler on a device pebbles-gpu created.
pub(crate) fn install_device_hooks(device: &wgpu::Device, health: &Arc<DeviceHealth>, init: &GpuInit) {
    let h = Arc::clone(health);
    let wake = init.on_device_lost.clone();
    device.set_device_lost_callback(move |reason, message| {
        // ANY thread (inside poll/submit, or a driver thread). Atomics + the stored message ONLY.
        h.mark_lost(reason);
        if let Ok(mut m) = h.lost_message.lock() {
            *m = Some(message);
        }
        if let Some(wake) = &wake {
            wake();
        }
    });
    let h = Arc::clone(health);
    let (strict, trace) = (init.strict_validation, init.gpu_trace);
    device.on_uncaptured_error(Arc::new(move |e: wgpu::Error| {
        route_uncaptured(&h, strict, trace, e);
    }));
}

/// Classify one uncaptured error (08 §6.3). Returns whether it was logged.
#[allow(clippy::panic)] // strict mode (PEBBLES_GPU_STRICT) panics by contract (11 §4.6)
pub(crate) fn route_uncaptured(h: &DeviceHealth, strict: bool, trace: bool, e: wgpu::Error) -> bool {
    let logged = match &e {
        wgpu::Error::OutOfMemory { .. } | wgpu::Error::Internal { .. } => {
            let n = h.fatal_errors.fetch_add(1, Relaxed) + 1;
            logf(
                Level::Error,
                Cat::Gpu,
                format_args!("fatal wgpu error #{n} (device poisoned — scheduling reset): {e}"),
            );
            true
        }
        wgpu::Error::Validation { description, source } => {
            let n = h.validation_errors.fetch_add(1, Relaxed) + 1;
            let key = {
                use std::hash::{Hash, Hasher};
                let mut s = std::hash::DefaultHasher::new();
                description.hash(&mut s);
                s.finish()
            };
            // Remember at most 64 distinct descriptions (11 §4.6); after that only the counter moves.
            let first = h.seen.lock().map(|mut set| set.len() < 64 && set.insert(key)).unwrap_or(true);
            if first {
                // `description` is often just "Validation Error"; the detail is in the source chain.
                let mut detail = String::new();
                let mut cause: Option<&(dyn std::error::Error + 'static)> = Some(source.as_ref());
                while let Some(c) = cause {
                    detail.push_str(": ");
                    detail.push_str(&c.to_string());
                    cause = c.source();
                }
                logf(
                    Level::Error,
                    Cat::Gpu,
                    format_args!("wgpu validation error #{n} (first occurrence): {description}{detail}"),
                );
            }
            first
        }
    };
    if trace {
        logf(Level::Error, Cat::Gpu, format_args!("{}", std::backtrace::Backtrace::force_capture()));
    }
    if strict {
        panic!("wgpu error (PEBBLES_GPU_STRICT): {e}");
    }
    logged
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
        Self { instance, init: init.clone(), devices: Vec::new() }
    }

    /// Create + configure a surface for `window` (on its compatible device).
    pub async fn create_surface<'window>(
        &mut self,
        window: impl Into<wgpu::SurfaceTarget<'window>>,
        width: u32,
        height: u32,
    ) -> Result<super::surface::RenderSurface<'window>, GpuError> {
        let surface = self.instance.create_surface(window).map_err(GpuError::from)?;
        super::surface::RenderSurface::configure_new(self, surface, width, height).await
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
            let Some(&best) = ranked.order.first() else {
                return Err(GpuError::NoAdapter {
                    backends: self.init.backends,
                    software_only: self.init.force_software,
                    enumerated,
                    source: None,
                });
            };
            self.finish_device(&adapters[best]).await
        }
    }

    /// Request the device for `adapter` with the portable limits profile, probe caps,
    /// log the §14.1 line and push the [`GpuDevice`].
    async fn finish_device(&mut self, adapter: &wgpu::Adapter) -> Result<DeviceId, GpuError> {
        let info = adapter.get_info();
        let limits = wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits());
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("pebbles.gpu.device"),
                required_features: wgpu::Features::empty(),
                required_limits: limits,
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                ..Default::default()
            })
            .await
            .map_err(|source| GpuError::RequestDevice { adapter: info.name.clone(), source })?;

        let downlevel = adapter.get_downlevel_capabilities();
        let surface_view_formats = downlevel.flags.contains(wgpu::DownlevelFlags::SURFACE_VIEW_FORMATS);
        let is_gl = info.backend == wgpu::Backend::Gl;
        let supports_4x =
            |f: wgpu::TextureFormat| adapter.get_texture_format_features(f).flags.sample_count_supported(4);
        let msaa_samples = pick_msaa(
            supports_4x(wgpu::TextureFormat::Bgra8Unorm) && supports_4x(wgpu::TextureFormat::Rgba8Unorm),
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
        let soft = if software { " | software" } else { "" };
        logf(
            Level::Info,
            Cat::Gpu,
            format_args!(
                "GPU: {} | backend={} type={:?} | driver=\"{} {}\" | max_tex={} | power={:?}{}",
                info.name,
                info.backend,
                info.device_type,
                info.driver,
                info.driver_info,
                caps.limits.max_texture_dimension_2d,
                self.init.power,
                soft
            ),
        );
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
pub(crate) mod tests {
    use super::*;

    #[test]
    fn msaa_is_four_or_one() {
        assert_eq!(pick_msaa(true), 4);
        assert_eq!(pick_msaa(false), 1);
    }

    #[test]
    #[ignore = "gpu"]
    fn limits_use_adapter_resolution() {
        let init = GpuInit::headless();
        let mut ctx = pollster::block_on(GpuContext::new(&init));
        let dev_id = match pollster::block_on(ctx.request_device(None)) {
            Ok(id) => id,
            Err(_) => {
                if std::env::var("PEBBLES_REQUIRE_GPU").is_ok_and(|v| v == "1") {
                    panic!("PEBBLES_REQUIRE_GPU=1 but no GPU adapter");
                }
                return;
            }
        };
        let dev = ctx.device(dev_id).expect("device");
        assert_eq!(
            dev.device().limits().max_texture_dimension_2d,
            dev.adapter().limits().max_texture_dimension_2d
        );
        assert_eq!(dev.device().limits().max_storage_buffers_per_shader_stage, 0);
    }

    /// Build a headless context + device, or `None` without an adapter (panics under
    /// `PEBBLES_REQUIRE_GPU=1`).
    pub(crate) fn gpu_or_skip(init: &GpuInit) -> Option<(GpuContext, DeviceId)> {
        let mut ctx = pollster::block_on(GpuContext::new(init));
        match pollster::block_on(ctx.request_device(None)) {
            Ok(id) => Some((ctx, id)),
            Err(_) => {
                assert!(
                    !std::env::var("PEBBLES_REQUIRE_GPU").is_ok_and(|v| v == "1"),
                    "PEBBLES_REQUIRE_GPU=1 but no GPU adapter"
                );
                None
            }
        }
    }

    fn validation(description: &str) -> wgpu::Error {
        wgpu::Error::Validation { source: Box::new(std::fmt::Error), description: description.to_owned() }
    }

    #[test]
    fn health_mark_lost() {
        let h = DeviceHealth::default();
        assert!(!h.is_poisoned());
        h.mark_lost(wgpu::DeviceLostReason::Unknown);
        assert!(h.is_poisoned() && h.is_lost());
        assert_eq!(h.lost_reason(), Some(wgpu::DeviceLostReason::Unknown));
    }

    #[test]
    fn routing_classifies_errors() {
        let h = DeviceHealth::default();
        assert!(route_uncaptured(&h, false, false, validation("bad view")));
        assert!(!route_uncaptured(&h, false, false, validation("bad view")), "same description logs once");
        assert_eq!(h.validation_errors(), 2);
        assert!(!h.is_poisoned(), "validation errors never poison");
        route_uncaptured(&h, false, false, wgpu::Error::OutOfMemory { source: Box::new(std::fmt::Error) });
        assert!(h.is_poisoned());
        assert_eq!(h.fatal_errors(), 1);
    }

    #[test]
    #[ignore = "gpu"]
    fn device_destroy_fires_lost_callback() {
        let woke = Arc::new(AtomicU64::new(0));
        let mut init = GpuInit::headless();
        let w = Arc::clone(&woke);
        init.on_device_lost = Some(Arc::new(move || {
            w.fetch_add(1, Relaxed);
        }));
        let Some((ctx, id)) = gpu_or_skip(&init) else { return };
        let dev = ctx.device(id).expect("device");
        dev.device().destroy();
        let _ = dev.device().poll(wgpu::PollType::Poll);
        assert_eq!(dev.health().lost_reason(), Some(wgpu::DeviceLostReason::Destroyed));
        assert!(ctx.any_device_poisoned());
        assert_eq!(ctx.lost_devices().len(), 1);
        assert_eq!(woke.load(Relaxed), 1);
    }

    #[test]
    #[ignore = "gpu"]
    fn strict_mode_panics_on_validation_error() {
        let mut init = GpuInit::headless();
        init.strict_validation = true;
        let Some((ctx, id)) = gpu_or_skip(&init) else { return };
        let device = ctx.device(id).expect("device").device().clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // A zero-size texture is a validation error.
            let _texture = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d { width: 0, height: 0, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
        }));
        let payload = result.expect_err("strict mode must panic");
        let msg = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|s| (*s).to_owned()))
            .unwrap_or_default();
        assert!(msg.contains("wgpu error (PEBBLES_GPU_STRICT)"), "{msg}");
    }
}
