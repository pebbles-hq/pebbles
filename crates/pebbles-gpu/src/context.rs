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
#[path = "context_tests.rs"]
pub(crate) mod tests;
