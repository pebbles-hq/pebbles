//! Surface configuration: the swapchain format and per-surface state.

use super::context::{DeviceId, GpuContext, GpuDevice};
use super::error::GpuError;

/// A configured presentation surface.
pub struct RenderSurface<'window> {
    pub(crate) surface: wgpu::Surface<'window>,
    pub(crate) config: wgpu::SurfaceConfiguration,
    dev_id: DeviceId,
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

    /// The render (view) format.
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    /// Reconfigure to `width`×`height` (clamped to `1..=max_texture_dimension_2d`).
    pub fn resize(&mut self, gpu: &GpuDevice, width: u32, height: u32) {
        let max = gpu.device().limits().max_texture_dimension_2d;
        self.config.width = width.clamp(1, max);
        self.config.height = height.clamp(1, max);
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
    ) -> Result<RenderSurface<'window>, GpuError> {
        let dev_id = ctx.request_device(Some(&surface)).await?;
        let device = ctx.device(dev_id).expect("invariant: request_device just created this device");
        let capabilities = surface.get_capabilities(device.adapter());
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or(GpuError::NoSurfaceFormat { offered: capabilities.formats.clone() })?;
        let alpha_mode = capabilities.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto);
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
