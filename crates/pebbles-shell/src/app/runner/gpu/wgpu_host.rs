//! The wgpu host: a thin adapter over `pebbles-gpu`, matching the shell's host seam.

use pebbles_gpu::{
    FrameParams, GpuContext, RenderSurface as GpuSurface, RenderTarget, Renderer as GpuRenderer,
};
use pebbles_render::Scene;
use peniko::Color;

use super::super::frame::FrameOutcome;

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
    ) -> Result<RenderSurface<'w>, HostError> {
        let surface = self.inner.create_surface(window, width, height).await.map_err(HostError)?;
        // pebbles-gpu installed its device hooks (`DeviceHealth`) in `request_device` (C18).
        Ok(RenderSurface(surface))
    }

    /// The instance surfaces are created with (08 §17.5: on the UI thread, in both modes).
    #[cfg_attr(target_family = "wasm", allow(dead_code))] // native surface replacement only
    pub(crate) fn instance(&self) -> wgpu::Instance {
        self.inner.instance()
    }

    /// Finish a surface created from [`instance`](Self::instance) (08 §17.2).
    #[cfg_attr(target_family = "wasm", allow(dead_code))] // native surface replacement only
    pub(crate) async fn adopt_surface(
        &mut self,
        surface: wgpu::Surface<'static>,
        size: (u32, u32),
    ) -> Result<RenderSurface<'static>, HostError> {
        Ok(RenderSurface(self.inner.adopt_surface(surface, size).await.map_err(HostError)?))
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

    /// Soak CSV (11 §2.9): `(allocated, reserved)` driver bytes of device `device`.
    pub(crate) fn allocator_report(&self, device: usize) -> Option<(u64, u64)> {
        let (_, gpu) = self.inner.devices().nth(device)?;
        let r = gpu.device().generate_allocator_report()?;
        Some((r.total_allocated_bytes, r.total_reserved_bytes))
    }

    /// Soak tool (`PEBBLES_SYNTH_GPU_LOSS`): destroy the real devices (P10).
    pub(crate) fn inject_synthetic_loss(&self) {
        self.inner.simulate_device_loss();
    }

    /// Acquire the swapchain image, render `scene` straight into it, present, and map every
    /// acquire outcome (08 §5.1, §17.3 (b)–(f)). No winit: the size and scale come from the
    /// packet (08 §17.1 T1). Reconfigures happen only after the frame is presented or dropped.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn present(
        &self,
        renderer: &mut Renderer,
        surface: &mut RenderSurface<'_>,
        scene: &Scene,
        base_color: Color,
        size: (u32, u32),
        scale_factor: f64,
        debug: impl Fn(&str),
    ) -> FrameOutcome {
        use pebbles_foundation::log::{Cat, Level, logf};
        use pebbles_gpu::{Acquire, FrameAction};
        let Some(gpu) = self.inner.device(surface.0.device_id()) else { return FrameOutcome::DeviceLost };
        let id = surface.0.device_id().index();
        let reconfigure = |surface: &mut RenderSurface<'_>| {
            surface.0.reconfigure(gpu, size.0.max(1), size.1.max(1));
        };
        // 08 §4.4 (P13): configure between frames when the packet's (clamped) size differs.
        surface.0.resize(gpu, size.0, size.1);
        let (frame, status) = match surface.0.acquire() {
            Acquire::Frame(frame) => {
                let status = frame.status();
                (frame, status)
            }
            // 08 §5.5 (P20): a streak of the same failure escalates (3 × Validation → full reset).
            Acquire::NotReady(e) => {
                return match surface.0.escalate(e.status()) {
                    FrameAction::SkipUntilVisible => FrameOutcome::Occluded,
                    FrameAction::RecreateSurfaceAndRedraw => FrameOutcome::SurfaceLost,
                    FrameAction::FullReset => FrameOutcome::DeviceLost,
                    FrameAction::ReconfigureAndRedraw => {
                        reconfigure(surface);
                        debug(&format!("surface #{id} {e:?} — reconfigured"));
                        FrameOutcome::Skipped
                    }
                    FrameAction::Render | FrameAction::RenderThenReconfigure | FrameAction::SkipAndRedraw => {
                        FrameOutcome::Skipped
                    }
                };
            }
        };
        let format = surface.0.view_format();
        // The size comes from the acquired texture (DR-12).
        let decode = surface.0.decode_srgb_output();
        let target =
            RenderTarget::new(frame.view(), format, surface.0.targets_mut()).with_decode_srgb_output(decode);
        // The device-pixel ratio drives the 02 §7.8 hinting policy (R27).
        let params = FrameParams::new(base_color).with_scale_factor(scale_factor);
        if let Err(e) = renderer.render(target, scene, &params) {
            // A lost or poisoned device reaches the reset through `needs_reset()` (C18).
            logf(Level::Error, Cat::Gpu, format_args!("wgpu render failed (frame skipped): {e}"));
            drop(frame);
            return FrameOutcome::Skipped;
        }
        let _ = gpu.device().poll(wgpu::PollType::Poll);
        frame.present();
        if surface.0.escalate(status) == FrameAction::RenderThenReconfigure {
            reconfigure(surface);
            debug(&format!("surface #{id} Suboptimal — reconfigured"));
            // The next frame catches up at the new configuration.
            return FrameOutcome::Skipped;
        }
        FrameOutcome::Presented
    }
}

/// A GPU setup failure: displays as `GpuError` (one line); `startup_failure_text` adds the
/// per-OS guidance (08 §14.3, P07).
#[derive(Debug)]
pub(crate) struct HostError(pebbles_gpu::GpuError);

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// `{error} — {guidance}` for recovery and secondary-window logs (08 §14.3).
pub(crate) fn error_text(e: &HostError) -> String {
    format!("{} — {}", e.0, e.0.guidance(pebbles_gpu::HostOs::current()))
}

/// The §14.3 guidance logged once when GPU resets keep failing (08 §6.7, P19).
pub(crate) fn repeated_reset_guidance() -> &'static str {
    pebbles_gpu::GpuError::DeviceLost.guidance(pebbles_gpu::HostOs::current())
}

/// The native startup failure text (08 §14.3).
#[cfg_attr(target_family = "wasm", allow(dead_code))] // native startup only
pub(crate) fn startup_failure_text(e: &HostError) -> String {
    format!(
        "\npebbles: could not initialize the GPU.\n  {}\n  {}\n",
        e.0,
        e.0.guidance(pebbles_gpu::HostOs::current())
    )
}

/// Thin wrapper over `pebbles_gpu::RenderSurface`.
pub(crate) struct RenderSurface<'w>(GpuSurface<'w>);

impl RenderSurface<'_> {
    pub(crate) fn device_index(&self) -> usize {
        self.0.device_id().index()
    }

    #[allow(dead_code)] // the seam contract; `present` sizes from the packet
    pub(crate) fn size(&self) -> (u32, u32) {
        self.0.size()
    }
}

pub(crate) type Renderer = GpuRenderer;

/// The `[pebbles gpu]` stats line (10 §9.1 `Display`), for `PEBBLES_FRAME_STATS` (06 §3.5 seam).
/// Soak CSV (11 §2.9): the renderer census (debug assertions only).
pub(crate) fn census_values(renderer: &Renderer) -> Option<[u64; 25]> {
    #[cfg(debug_assertions)]
    return Some(renderer.census().values());
    #[cfg(not(debug_assertions))]
    {
        let _ = renderer;
        None
    }
}

pub(crate) fn frame_stats_line(renderer: &Renderer) -> Option<String> {
    Some(renderer.last_frame_stats().to_string())
}
