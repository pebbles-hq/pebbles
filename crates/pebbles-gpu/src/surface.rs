//! Surface configuration: the swapchain format and per-surface state.

use std::sync::OnceLock;

use super::context::{DeviceId, GpuContext, GpuDevice};

/// The swapchain texture format chosen at surface creation. Captured once so the
/// lazily-built solid pipeline targets the right format (deleted by C06).
pub(crate) static SURFACE_FORMAT: OnceLock<wgpu::TextureFormat> = OnceLock::new();

/// A configured presentation surface.
pub struct RenderSurface<'window> {
    pub(crate) surface: wgpu::Surface<'window>,
    pub(crate) config: wgpu::SurfaceConfiguration,
    dev_id: DeviceId,
    #[allow(dead_code)]
    format: wgpu::TextureFormat,
}

impl<'window> RenderSurface<'window> {
    /// The surface's device id.
    pub fn device_id(&self) -> DeviceId {
        self.dev_id
    }

    /// The configured size (physical pixels).
    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Reconfigure to `width`×`height` (clamped to `>= 1`).
    pub fn resize(&mut self, gpu: &GpuDevice, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(gpu.device(), &self.config);
    }

    /// The underlying wgpu surface (for the shell's acquire/present).
    pub fn surface(&self) -> &wgpu::Surface<'window> {
        &self.surface
    }

    pub(crate) async fn configure_new(
        ctx: &mut GpuContext,
        surface: wgpu::Surface<'window>,
        width: u32,
        height: u32,
        present_mode: wgpu::PresentMode,
    ) -> Result<RenderSurface<'window>, String> {
        let dev_id = ctx.request_device(Some(&surface)).await.ok_or("no compatible GPU device")?;
        let device = ctx.device(dev_id).ok_or("no compatible GPU device")?;
        let capabilities = surface.get_capabilities(device.adapter());
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or("surface exposes no supported texture format")?;
        let alpha_mode = capabilities.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto);
        SURFACE_FORMAT.get_or_init(|| format);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![],
        };
        surface.configure(device.device(), &config);
        Ok(RenderSurface { surface, config, dev_id, format })
    }
}
