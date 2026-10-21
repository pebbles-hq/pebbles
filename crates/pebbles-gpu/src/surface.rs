//! Surface configuration: the swapchain format choice and per-surface state.

use super::context::{DeviceId, GpuContext, GpuDevice};
use super::error::GpuError;
use crate::error::AcquireError;
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

/// The acquire result without the texture (08 §5.2), for the pure mapping and for logging.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcquireStatus {
    /// A frame is ready.
    Success,
    /// A frame is ready, but the surface should be reconfigured after presenting it.
    Suboptimal,
    /// wgpu already waited 1 s for a frame.
    Timeout,
    /// The window is not visible.
    Occluded,
    /// The surface no longer matches the window.
    Outdated,
    /// The surface is gone and must be recreated.
    Lost,
    /// Acquiring raised an error (possibly a lost device).
    Validation,
}

/// What the frame loop does for an [`AcquireStatus`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameAction {
    /// Render and present.
    Render,
    /// Render and present, then reconfigure (texture dropped) and redraw.
    RenderThenReconfigure,
    /// No render; request a redraw.
    SkipAndRedraw,
    /// No render; mark the window occluded and wait for a visibility event.
    SkipUntilVisible,
    /// Reconfigure at the window's size and redraw.
    ReconfigureAndRedraw,
    /// Recreate the surface and redraw.
    RecreateSurfaceAndRedraw,
    /// Rebuild the whole GPU stack (P20 escalation only).
    FullReset,
}

/// The 08 §5.1 table.
pub const fn action_for(status: AcquireStatus) -> FrameAction {
    match status {
        AcquireStatus::Success => FrameAction::Render,
        AcquireStatus::Suboptimal => FrameAction::RenderThenReconfigure,
        AcquireStatus::Timeout => FrameAction::SkipAndRedraw,
        AcquireStatus::Occluded => FrameAction::SkipUntilVisible,
        AcquireStatus::Outdated | AcquireStatus::Validation => FrameAction::ReconfigureAndRedraw,
        AcquireStatus::Lost => FrameAction::RecreateSurfaceAndRedraw,
    }
}

impl AcquireError {
    /// The status this error reports.
    pub const fn status(self) -> AcquireStatus {
        match self {
            AcquireError::Timeout => AcquireStatus::Timeout,
            AcquireError::Occluded => AcquireStatus::Occluded,
            AcquireError::Outdated => AcquireStatus::Outdated,
            AcquireError::Lost => AcquireStatus::Lost,
            AcquireError::Validation => AcquireStatus::Validation,
        }
    }
}

/// One acquired swapchain frame, viewed in the surface's render format.
pub struct SurfaceFrame {
    texture: wgpu::SurfaceTexture,
    view: wgpu::TextureView,
    suboptimal: bool,
}

impl SurfaceFrame {
    /// The view to render into (the surface's view format).
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// The status this frame was acquired with (`Success` or `Suboptimal`).
    pub fn status(&self) -> AcquireStatus {
        if self.suboptimal { AcquireStatus::Suboptimal } else { AcquireStatus::Success }
    }

    /// Present the frame (consumes it, so reconfiguring afterwards is safe — 08 §5.3 rule 1).
    pub fn present(self) {
        drop(self.view);
        self.texture.present();
    }
}

/// The outcome of [`RenderSurface::acquire`].
pub enum Acquire {
    /// A frame to render into.
    Frame(SurfaceFrame),
    /// No frame this time.
    NotReady(AcquireError),
}

/// A configured presentation surface.
pub struct RenderSurface<'window> {
    pub(crate) surface: wgpu::Surface<'window>,
    pub(crate) config: wgpu::SurfaceConfiguration,
    dev_id: DeviceId,
    view_format: wgpu::TextureFormat,
    /// The encoding contract the shaders must honor (R13 reads it into `Globals::color_flags`).
    pub(crate) encoding: OutputEncoding,
    /// This surface's own targets (the Tier C / layer pool, 10 §4, E3).
    targets: FrameTargets,
}

/// 08 §4.2: Pebbles has no transparent windows — `Opaque` when offered, else `Auto`.
pub fn choose_alpha_mode(modes: &[wgpu::CompositeAlphaMode]) -> wgpu::CompositeAlphaMode {
    if modes.contains(&wgpu::CompositeAlphaMode::Opaque) {
        wgpu::CompositeAlphaMode::Opaque
    } else {
        wgpu::CompositeAlphaMode::Auto
    }
}

/// 08 §4.3: the mode to configure and whether `wanted` had to be replaced.
pub fn choose_present_mode(
    wanted: wgpu::PresentMode,
    modes: &[wgpu::PresentMode],
) -> (wgpu::PresentMode, bool) {
    use wgpu::PresentMode::{AutoNoVsync, AutoVsync, Fifo};
    match wanted {
        // Resolved by wgpu itself against the surface; always valid. Fifo is always offered.
        AutoVsync | AutoNoVsync | Fifo => (wanted, false),
        m if modes.contains(&m) => (m, false),
        _ => (AutoVsync, true),
    }
}

/// 08 §4.3: `desired_maximum_frame_latency` clamped to 1..=3.
pub const fn frame_latency(requested: u32) -> u32 {
    if requested < 1 {
        1
    } else if requested > 3 {
        3
    } else {
        requested
    }
}

/// 08 §4.4: `None` = do not configure (0-sized); else the clamped size and whether it clamped.
pub fn clamp_surface_size(width: u32, height: u32, max_dim: u32) -> Option<((u32, u32), bool)> {
    if width == 0 || height == 0 {
        return None;
    }
    let (w, h) = (width.min(max_dim), height.min(max_dim));
    Some(((w, h), w != width || h != height))
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

    /// Acquire the next frame — the ONLY place Pebbles matches `CurrentSurfaceTexture` (08 §5.2).
    pub fn acquire(&mut self) -> Acquire {
        let (texture, suboptimal) = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => (t, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => (t, true),
            wgpu::CurrentSurfaceTexture::Timeout => return Acquire::NotReady(AcquireError::Timeout),
            wgpu::CurrentSurfaceTexture::Occluded => return Acquire::NotReady(AcquireError::Occluded),
            wgpu::CurrentSurfaceTexture::Outdated => return Acquire::NotReady(AcquireError::Outdated),
            wgpu::CurrentSurfaceTexture::Lost => return Acquire::NotReady(AcquireError::Lost),
            wgpu::CurrentSurfaceTexture::Validation => return Acquire::NotReady(AcquireError::Validation),
        };
        let view = texture.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.view_format),
            ..Default::default()
        });
        Acquire::Frame(SurfaceFrame { texture, view, suboptimal })
    }

    /// Reconfigure at `width`×`height` even when the size is unchanged (Outdated/Suboptimal).
    /// Never call while a [`SurfaceFrame`] of this surface is alive.
    pub fn reconfigure(&mut self, gpu: &GpuDevice, width: u32, height: u32) {
        self.resize(gpu, width, height);
    }

    /// Whether frames must decode sRGB in the shader (GL sRGB-only surface, 02 §3.8): pass to
    /// [`RenderTarget::with_decode_srgb_output`](crate::RenderTarget::with_decode_srgb_output).
    pub fn decode_srgb_output(&self) -> bool {
        self.encoding == OutputEncoding::SrgbHardware
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
                format_args!(
                    "sRGB-only surface without view formats — colors decoded in the shader (R13); opaque colors match, translucent pixels, AA edges and glyph coverage blend in linear light (documented deviation, 02 §3.8)"
                ),
            );
        }
        let alpha_mode = choose_alpha_mode(&capabilities.alpha_modes);
        let wanted = ctx.init().present_mode;
        let (present_mode, replaced) = choose_present_mode(wanted, &capabilities.present_modes);
        if replaced {
            pebbles_foundation::log::logf(
                pebbles_foundation::log::Level::Warn,
                pebbles_foundation::log::Cat::Gpu,
                format_args!(
                    "present mode {wanted:?} unsupported by this surface ({:?}); using AutoVsync",
                    capabilities.present_modes
                ),
            );
        }
        let max = device.device().limits().max_texture_dimension_2d;
        // A 0-sized window at creation configures at 1×1; the first real resize reconfigures.
        let (width, height) = match clamp_surface_size(width, height, max) {
            None => (1, 1),
            Some(((w, h), clamped)) => {
                if clamped {
                    pebbles_foundation::log::logf(
                        pebbles_foundation::log::Level::Warn,
                        pebbles_foundation::log::Cat::Gpu,
                        format_args!(
                            "surface {width}x{height} exceeds max texture size {max}; rendering at {w}x{h} (stretched)"
                        ),
                    );
                }
                (w, h)
            }
        };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: choice.format,
            width,
            height,
            present_mode,
            desired_maximum_frame_latency: frame_latency(ctx.init().max_frame_latency),
            alpha_mode,
            // Only list the view format when it differs from the surface format (GL — DL-10).
            view_formats: if choice.view_format != choice.format { vec![choice.view_format] } else { vec![] },
        };
        surface.configure(device.device(), &config);
        let line = super::diag::surface_line(&super::diag::SurfaceSummary {
            format: config.format,
            view_format: choice.view_format,
            encoding: choice.encoding,
            present: config.present_mode,
            latency: config.desired_maximum_frame_latency,
            alpha: config.alpha_mode,
            width: config.width,
            height: config.height,
        });
        pebbles_foundation::log::logf(
            pebbles_foundation::log::Level::Debug,
            pebbles_foundation::log::Cat::Gpu,
            format_args!("{line}"),
        );
        if ctx.init().print_info || cfg!(target_family = "wasm") {
            super::diag::stderr_line(&line);
        }
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

    #[test]
    fn alpha_prefers_opaque() {
        use wgpu::CompositeAlphaMode as A;
        assert_eq!(choose_alpha_mode(&[A::PreMultiplied, A::Opaque]), A::Opaque);
        assert_eq!(choose_alpha_mode(&[A::Inherit]), A::Auto);
    }

    #[test]
    fn present_mode_fallback() {
        use wgpu::PresentMode as P;
        assert_eq!(choose_present_mode(P::Mailbox, &[P::Fifo]), (P::AutoVsync, true));
        assert_eq!(choose_present_mode(P::Immediate, &[P::Fifo, P::Immediate]), (P::Immediate, false));
        assert_eq!(choose_present_mode(P::Fifo, &[]), (P::Fifo, false));
    }

    #[test]
    fn latency_clamped() {
        assert_eq!((frame_latency(0), frame_latency(2), frame_latency(9)), (1, 2, 3));
    }

    #[test]
    fn size_clamp() {
        assert_eq!(clamp_surface_size(0, 10, 8192), None);
        assert_eq!(clamp_surface_size(9000, 100, 8192), Some(((8192, 100), true)));
    }

    #[test]
    fn action_for_every_status() {
        use AcquireStatus as S;
        use FrameAction as A;
        let table = [
            (S::Success, A::Render),
            (S::Suboptimal, A::RenderThenReconfigure),
            (S::Timeout, A::SkipAndRedraw),
            (S::Occluded, A::SkipUntilVisible),
            (S::Outdated, A::ReconfigureAndRedraw),
            (S::Lost, A::RecreateSurfaceAndRedraw),
            (S::Validation, A::ReconfigureAndRedraw),
        ];
        for (status, action) in table {
            assert_eq!(action_for(status), action, "{status:?}");
        }
        for e in [
            AcquireError::Timeout,
            AcquireError::Occluded,
            AcquireError::Outdated,
            AcquireError::Lost,
            AcquireError::Validation,
        ] {
            assert_ne!(e.status(), S::Success, "a NotReady is never a frame");
        }
    }

    /// Suboptimal still renders and presents; the reconfigure comes after the frame.
    #[test]
    fn suboptimal_presents_then_reconfigures() {
        let action = action_for(AcquireStatus::Suboptimal);
        assert_eq!(action, FrameAction::RenderThenReconfigure);
        assert_ne!(action, FrameAction::ReconfigureAndRedraw, "must not drop a usable frame");
    }
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
