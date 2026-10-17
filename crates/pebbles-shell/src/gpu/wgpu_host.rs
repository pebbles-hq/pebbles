//! The wgpu host: a thin adapter over `pebbles-gpu`, matching the shell's host seam.

use pebbles_gpu::{
    FrameParams, GpuContext, RenderSurface as GpuSurface, RenderTarget, Renderer as GpuRenderer,
};
use pebbles_render::Scene;
use peniko::Color;

use super::{GPU_ERRORS, PresentOutcome, install_error_handler};

/// Thin wrapper over `pebbles_gpu::GpuContext`.
pub(crate) struct RenderContext {
    inner: GpuContext,
    errors_seen: u64,
}

impl RenderContext {
    pub(crate) async fn new(init: &pebbles_gpu::GpuInit) -> Self {
        Self {
            inner: GpuContext::new(init).await,
            errors_seen: GPU_ERRORS.load(std::sync::atomic::Ordering::Relaxed),
        }
    }

    pub(crate) async fn create_surface<'w>(
        &mut self,
        window: impl Into<wgpu::SurfaceTarget<'w>>,
        width: u32,
        height: u32,
    ) -> Result<RenderSurface<'w>, String> {
        let surface = self.inner.create_surface(window, width, height).await.map_err(|e| e.to_string())?;
        let device = surface.device_id();
        install_error_handler(self.inner.device(device).expect("device just created").device());
        Ok(RenderSurface(surface))
    }

    pub(crate) fn resize_surface(&self, surface: &mut RenderSurface<'_>, width: u32, height: u32) {
        let gpu = self.inner.device(surface.0.device_id()).expect("device");
        surface.0.resize(gpu, width, height);
    }

    pub(crate) fn device_count(&self) -> usize {
        self.inner.devices().count()
    }

    pub(crate) fn new_renderer(&self, device: usize) -> Option<Renderer> {
        let (id, gpu) = self.inner.devices().nth(device)?;
        Some(GpuRenderer::new(gpu, id))
    }

    #[allow(dead_code)] // first caller: C18 (08 §6.5 step 1)
    pub(crate) fn needs_reset(&self) -> bool {
        GPU_ERRORS.load(std::sync::atomic::Ordering::Relaxed) != self.errors_seen
    }
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

/// Acquire the swapchain image, render `scene` straight into it, and present.
pub(crate) fn present_frame(
    ctx: &RenderContext,
    renderer: &mut Renderer,
    surface: &mut RenderSurface<'_>,
    scene: &Scene,
    base_color: Color,
    window: &winit::window::Window,
) -> PresentOutcome {
    let Some(gpu) = ctx.inner.device(surface.0.device_id()) else { return PresentOutcome::Skipped };
    let device = gpu.device();
    let surface_texture = match surface.0.surface().get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
        _ => return PresentOutcome::Skipped,
    };
    let format = surface.0.view_format();
    let view = surface_texture
        .texture
        .create_view(&wgpu::TextureViewDescriptor { format: Some(format), ..Default::default() });
    // The size comes from the acquired texture, never from the window (DR-12).
    let target = RenderTarget::new(&view, format, surface.0.targets_mut());
    if let Err(e) = renderer.render(target, scene, &FrameParams::new(base_color)) {
        eprintln!("pebbles: wgpu render failed (skipping frame, scheduling GPU reset): {e}");
        GPU_ERRORS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        window.request_redraw();
        return PresentOutcome::Skipped;
    }
    let _ = device.poll(wgpu::PollType::Poll);
    window.pre_present_notify();
    surface_texture.present();
    PresentOutcome::Presented
}
