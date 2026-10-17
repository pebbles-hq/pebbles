//! Surface configuration: the swapchain format choice and per-surface state.

use super::context::{DeviceId, GpuContext, GpuDevice};
use super::error::GpuError;
use crate::frame_targets::FrameTargets;

/// How the renderer encodes colors before the hardware stores them (08 §4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputEncoding {
    /// Shaders write sRGB-encoded (gamma) premultiplied values into a Unorm view. The norm.
    Gamma,
    /// The only writable view is sRGB: shaders convert colors sRGB→linear before output so the
    /// hardware re-encode restores the gamma values. Spec deviation (documented), logged once.
    SrgbHardware,
}

/// The surface format choice: what the surface is configured with, what the renderer draws into,
/// and the encoding contract that implies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceFormatChoice {
    /// `SurfaceConfiguration::format`.
    pub format: wgpu::TextureFormat,
    /// The format of the view the renderer draws into (Unorm unless `SrgbHardware`).
    pub view_format: wgpu::TextureFormat,
    /// The encoding contract the shaders must honor.
    pub encoding: OutputEncoding,
}

/// Pick the surface format (08 §4.1; DR-05). Prefer `Bgra8Unorm`/`Rgba8Unorm`; only fall back
/// to the sRGB formats when no Unorm format is offered.
pub fn choose_surface_format(
    formats: &[wgpu::TextureFormat],
    surface_view_formats: bool,
) -> Option<SurfaceFormatChoice> {
    use wgpu::TextureFormat::{Bgra8Unorm, Bgra8UnormSrgb, Rgba8Unorm, Rgba8UnormSrgb};
    for unorm in [Bgra8Unorm, Rgba8Unorm] {
        if formats.contains(&unorm) {
            return Some(SurfaceFormatChoice {
                format: unorm,
                view_format: unorm,
                encoding: OutputEncoding::Gamma,
            });
        }
    }
    let srgb = formats.iter().copied().find(|f| matches!(f, Bgra8UnormSrgb | Rgba8UnormSrgb))?;
    if surface_view_formats {
        let unorm = srgb.remove_srgb_suffix();
        Some(SurfaceFormatChoice { format: srgb, view_format: unorm, encoding: OutputEncoding::Gamma })
    } else {
        Some(SurfaceFormatChoice { format: srgb, view_format: srgb, encoding: OutputEncoding::SrgbHardware })
    }
}

/// A configured presentation surface.
pub struct RenderSurface<'window> {
    pub(crate) surface: wgpu::Surface<'window>,
    pub(crate) config: wgpu::SurfaceConfiguration,
    dev_id: DeviceId,
    view_format: wgpu::TextureFormat,
    /// The encoding contract the shaders must honor (R13 reads it into `Globals::color_flags`).
    #[allow(dead_code)] // first consumer: R13 (after C12)
    pub(crate) encoding: OutputEncoding,
    /// This surface's own MSAA target (10 §4, E3).
    targets: FrameTargets,
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

    /// The render (view) format the renderer draws into.
    pub fn view_format(&self) -> wgpu::TextureFormat {
        self.view_format
    }

    /// Reconfigure to `width`×`height` (clamped to `1..=max_texture_dimension_2d`).
    pub fn resize(&mut self, gpu: &GpuDevice, width: u32, height: u32) {
        let max = gpu.device().limits().max_texture_dimension_2d;
        self.config.width = width.clamp(1, max);
        self.config.height = height.clamp(1, max);
        self.surface.configure(gpu.device(), &self.config);
    }

    /// This surface's target textures, for `RenderTarget::new`.
    pub fn targets_mut(&mut self) -> &mut FrameTargets {
        &mut self.targets
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
    ) -> Result<RenderSurface<'window>, GpuError> {
        let dev_id = ctx.request_device(Some(&surface)).await?;
        let device = ctx.device(dev_id).ok_or(GpuError::DeviceLost)?;
        let capabilities = surface.get_capabilities(device.adapter());
        let surface_view_formats = device
            .adapter()
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::SURFACE_VIEW_FORMATS);
        let choice = choose_surface_format(&capabilities.formats, surface_view_formats)
            .ok_or(GpuError::NoSurfaceFormat { offered: capabilities.formats.clone() })?;
        if choice.encoding == OutputEncoding::SrgbHardware {
            pebbles_foundation::log::logf(
                pebbles_foundation::log::Level::Warn,
                pebbles_foundation::log::Cat::Gpu,
                format_args!("sRGB-only surface — colors decoded in the shader (R13)"),
            );
        }
        let alpha_mode = capabilities.alpha_modes.first().copied().unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: choice.format,
            width: width.max(1),
            height: height.max(1),
            present_mode: ctx.init().present_mode,
            desired_maximum_frame_latency: ctx.init().max_frame_latency,
            alpha_mode,
            // Only list the view format when it differs from the surface format (GL — DL-10).
            view_formats: if choice.view_format != choice.format { vec![choice.view_format] } else { vec![] },
        };
        surface.configure(device.device(), &config);
        Ok(RenderSurface {
            surface,
            config,
            dev_id,
            view_format: choice.view_format,
            encoding: choice.encoding,
            targets: FrameTargets::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::TextureFormat::{Bgra8Unorm, Bgra8UnormSrgb, Rgba8Unorm, Rgba8UnormSrgb, Rgba16Float};

    fn choice(formats: &[wgpu::TextureFormat], view_formats: bool) -> Option<SurfaceFormatChoice> {
        choose_surface_format(formats, view_formats)
    }

    #[test]
    fn format_webgpu_list() {
        let c = choice(&[Bgra8Unorm, Rgba8Unorm, Rgba16Float], false).unwrap();
        assert_eq!(c.format, Bgra8Unorm);
        assert_eq!(c.view_format, Bgra8Unorm);
        assert_eq!(c.encoding, OutputEncoding::Gamma);
    }

    #[test]
    fn format_webgpu_rgba_preferred() {
        let c = choice(&[Rgba8Unorm, Bgra8Unorm, Rgba16Float], false).unwrap();
        assert_eq!(c.format, Bgra8Unorm);
        assert_eq!(c.view_format, Bgra8Unorm);
        assert_eq!(c.encoding, OutputEncoding::Gamma);
    }

    #[test]
    fn format_webgl_list() {
        let c = choice(&[Rgba8Unorm, Rgba8UnormSrgb], false).unwrap();
        assert_eq!(c.format, Rgba8Unorm);
        assert_eq!(c.encoding, OutputEncoding::Gamma);
    }

    #[test]
    fn format_vulkan_srgb_first() {
        let c = choice(&[Bgra8UnormSrgb, Bgra8Unorm], false).unwrap();
        assert_eq!(c.format, Bgra8Unorm);
        assert_eq!(c.view_format, c.format);
        assert_eq!(c.encoding, OutputEncoding::Gamma);
    }

    #[test]
    fn format_srgb_only_with_view_formats() {
        let c = choice(&[Bgra8UnormSrgb], true).unwrap();
        assert_eq!(c.format, Bgra8UnormSrgb);
        assert_eq!(c.view_format, Bgra8Unorm);
        assert_eq!(c.encoding, OutputEncoding::Gamma);
    }

    #[test]
    fn format_srgb_only_without_view_formats() {
        let c = choice(&[Rgba8UnormSrgb], false).unwrap();
        assert_eq!(c.format, Rgba8UnormSrgb);
        assert_eq!(c.view_format, Rgba8UnormSrgb);
        assert_eq!(c.encoding, OutputEncoding::SrgbHardware);
    }

    #[test]
    fn format_none() {
        assert!(choice(&[], false).is_none());
        assert!(choice(&[Rgba16Float], false).is_none());
    }
}
