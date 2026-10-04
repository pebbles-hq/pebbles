//! The wgpu instance, adapter and device pool.

use super::error::GpuError;

/// Index of a device in its [`GpuContext`]. Invalid after the context is dropped/rebuilt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DeviceId(usize);

impl DeviceId {
    /// The underlying `usize` index.
    pub fn index(self) -> usize {
        self.0
    }
}

/// One adapter + device + queue.
pub struct GpuDevice {
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
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
}

/// The wgpu instance + device pool. Rebuilt wholesale on device loss (C18).
pub struct GpuContext {
    instance: wgpu::Instance,
    devices: Vec<GpuDevice>,
}

impl GpuContext {
    /// Create a context from `wgpu::Instance::default()` (env-aware init is C21).
    pub fn new() -> Self {
        Self { instance: wgpu::Instance::default(), devices: Vec::new() }
    }

    /// Create + configure a surface for `window` (on its compatible device).
    pub async fn create_surface<'window>(
        &mut self,
        window: impl Into<wgpu::SurfaceTarget<'window>>,
        width: u32,
        height: u32,
        present_mode: wgpu::PresentMode,
    ) -> Result<super::surface::RenderSurface<'window>, GpuError> {
        let surface = self.instance.create_surface(window).map_err(GpuError::from)?;
        super::surface::RenderSurface::configure_new(self, surface, width, height, present_mode).await
    }

    /// Reuse a compatible device, or request a new one.
    pub async fn request_device(
        &mut self,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<DeviceId, GpuError> {
        if let Some(index) = compatible_surface.and_then(|surface| {
            self.devices.iter().position(|device| device.adapter.is_surface_supported(surface))
        }) {
            return Ok(DeviceId(index));
        }
        if compatible_surface.is_none() && !self.devices.is_empty() {
            return Ok(DeviceId(0));
        }
        let adapter = self
            .instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|source| GpuError::NoAdapter {
                backends: wgpu::Backends::all(),
                software_only: false,
                enumerated: 0,
                source: Some(source),
            })?;
        let info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("pebbles.gpu.device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await
            .map_err(|source| GpuError::RequestDevice { adapter: info.name.clone(), source })?;
        self.devices.push(GpuDevice { adapter, device, queue });
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
}

impl Default for GpuContext {
    fn default() -> Self {
        Self::new()
    }
}
