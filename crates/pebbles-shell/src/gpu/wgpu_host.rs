//! The wgpu host: a thin adapter over `pebbles-gpu`, matching the shell's host seam.

use pebbles_gpu::{
    FrameParams, GpuContext, RenderSurface as GpuSurface, RenderTarget, Renderer as GpuRenderer,
};
use pebbles_render::Scene;
use peniko::Color;

use super::PresentOutcome;

/// Thin wrapper over `pebbles_gpu::GpuContext`.
pub(crate) struct RenderContext {
    inner: GpuContext,
}

impl RenderContext {
    pub(crate) async fn new(init: &pebbles_gpu::GpuInit) -> Self {
        Self { inner: GpuContext::new(init).await }
    }

    pub(crate) async fn create_surface<'w>(
        &mut self,
        window: impl Into<wgpu::SurfaceTarget<'w>>,
        width: u32,
        height: u32,
    ) -> Result<RenderSurface<'w>, String> {
        let surface = self.inner.create_surface(window, width, height).await.map_err(|e| e.to_string())?;
        // pebbles-gpu installed its device hooks (`DeviceHealth`) in `request_device` (C18).
        Ok(RenderSurface(surface))
    }

    pub(crate) fn resize_surface(&self, surface: &mut RenderSurface<'_>, width: u32, height: u32) {
        if let Some(gpu) = self.inner.device(surface.0.device_id()) {
            surface.0.resize(gpu, width, height);
        }
    }

    pub(crate) fn device_count(&self) -> usize {
        self.inner.devices().count()
    }

    pub(crate) fn new_renderer(&self, device: usize) -> Option<Renderer> {
        let (id, gpu) = self.inner.devices().nth(device)?;
        Some(GpuRenderer::new(gpu, id))
    }

    /// Any device lost or hit a fatal error (08 §6.5 step 1).
    pub(crate) fn needs_reset(&self) -> bool {
        self.inner.any_device_poisoned()
    }

    /// One line per lost device: `reason=…: message` (08 §6.5 step 3).
    pub(crate) fn lost_devices(&self) -> Vec<String> {
        self.inner
            .lost_devices()
            .into_iter()
            .map(|(_, reason, message)| format!("reason={reason:?}: {message}"))
            .collect()
    }

    /// Soak tool (`PEBBLES_SYNTH_GPU_LOSS`): a no-op until P10 destroys the devices.
    pub(crate) fn inject_synthetic_loss(&self) {}
}

/// Thin wrapper over `pebbles_gpu::RenderSurface`.
pub(crate) struct RenderSurface<'w>(GpuSurface<'w>);

impl RenderSurface<'_> {
    pub(crate) fn device_index(&self) -> usize {
        self.0.device_id().index()
    }

    #[allow(dead_code)] // the seam contract; present_frame sizes from the acquired texture
    pub(crate) fn size(&self) -> (u32, u32) {
        self.0.size()
    }
}

pub(crate) type Renderer = GpuRenderer;

/// Acquire the swapchain image, render `scene` straight into it, present, and act on every
/// acquire outcome (08 §5.1). Reconfigures happen only after the frame is presented or dropped.
pub(crate) fn present_frame(
    ctx: &RenderContext,
    renderer: &mut Renderer,
    surface: &mut RenderSurface<'_>,
    scene: &Scene,
    base_color: Color,
    window: &winit::window::Window,
) -> PresentOutcome {
    use pebbles_foundation::log::{Cat, Level, logf};
    use pebbles_gpu::{Acquire, FrameAction, action_for};
    // 08 §5.3 rule 3: never acquire on a dead device.
    if ctx.needs_reset() {
        return PresentOutcome::ResetGpu;
    }
    let Some(gpu) = ctx.inner.device(surface.0.device_id()) else { return PresentOutcome::ResetGpu };
    let id = surface.0.device_id().index();
    let reconfigure = |surface: &mut RenderSurface<'_>| {
        let size = window.inner_size();
        surface.0.reconfigure(gpu, size.width.max(1), size.height.max(1));
    };
    let (frame, status) = match surface.0.acquire() {
        Acquire::Frame(frame) => {
            let status = frame.status();
            (frame, status)
        }
        Acquire::NotReady(e) => {
            return match action_for(e.status()) {
                FrameAction::SkipUntilVisible => PresentOutcome::Occluded,
                FrameAction::RecreateSurfaceAndRedraw => PresentOutcome::RecreateSurface,
                FrameAction::FullReset => PresentOutcome::ResetGpu,
                FrameAction::ReconfigureAndRedraw => {
                    reconfigure(surface);
                    logf(Level::Debug, Cat::Gpu, format_args!("surface #{id} {e:?} — reconfigured"));
                    window.request_redraw();
                    PresentOutcome::RedrawRequested
                }
                FrameAction::Render | FrameAction::RenderThenReconfigure | FrameAction::SkipAndRedraw => {
                    window.request_redraw();
                    PresentOutcome::RedrawRequested
                }
            };
        }
    };
    let format = surface.0.view_format();
    // The size comes from the acquired texture, never from the window (DR-12).
    let decode = surface.0.decode_srgb_output();
    let target =
        RenderTarget::new(frame.view(), format, surface.0.targets_mut()).with_decode_srgb_output(decode);
    if let Err(e) = renderer.render(target, scene, &FrameParams::new(base_color)) {
        // A lost or poisoned device reaches the reset through `needs_reset()` (C18).
        logf(Level::Error, Cat::Gpu, format_args!("wgpu render failed (frame skipped): {e}"));
        drop(frame);
        window.request_redraw();
        return PresentOutcome::RedrawRequested;
    }
    let _ = gpu.device().poll(wgpu::PollType::Poll);
    window.pre_present_notify();
    frame.present();
    if action_for(status) == FrameAction::RenderThenReconfigure {
        reconfigure(surface);
        logf(Level::Debug, Cat::Gpu, format_args!("surface #{id} Suboptimal — reconfigured"));
        window.request_redraw();
    }
    PresentOutcome::Presented
}
