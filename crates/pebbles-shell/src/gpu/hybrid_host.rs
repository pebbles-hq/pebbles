//! The Vello Hybrid GPU host. It reuses `vello::util` for the backend-agnostic parts — the
//! wgpu instance/adapter/device pool (`RenderContext`) and the swapchain surface — and
//! rasterizes **straight into the swapchain** (no intermediate/blit) by flushing the
//! recorded op-list into a `vello_hybrid::Scene` and rendering it directly.

use std::collections::HashMap;
use std::ops::Deref;
use std::sync::OnceLock;

use vello::util::RenderContext as VelloCtx;

use vello_hybrid::{
    RenderSize, RenderTargetConfig, Renderer as HybRenderer, Resources, Scene as HybScene, TextureBindings,
    TextureId,
};

use pebbles_render::Scene;
use peniko::Color;

use super::PresentOutcome;

/// The swapchain texture format chosen by `vello::util`. Captured once at surface creation.
static SURFACE_FORMAT: OnceLock<wgpu::TextureFormat> = OnceLock::new();

/// Antialiasing selector — the hybrid backend antialiases internally.
#[derive(Clone, Copy, Debug)]
pub(crate) enum AaConfig {
    Area,
}

/// Per-frame render parameters — same fields the runner fills for the wgpu host.
pub(crate) struct RenderParams {
    pub base_color: Color,
    pub width: u32,
    pub height: u32,
    #[allow(dead_code)]
    pub antialiasing_method: AaConfig,
}

/// The wgpu context — a thin wrapper over `vello::util::RenderContext`.
pub(crate) struct RenderContext {
    inner: VelloCtx,
    errors_seen: u64,
}

impl RenderContext {
    pub(crate) fn new() -> Self {
        Self { inner: VelloCtx::new(), errors_seen: GPU_ERRORS.load(std::sync::atomic::Ordering::Relaxed) }
    }

    pub(crate) async fn create_surface<'w>(
        &mut self,
        window: impl Into<wgpu::SurfaceTarget<'w>>,
        width: u32,
        height: u32,
    ) -> Result<RenderSurface<'w>, vello::Error> {
        let surface = self.inner.create_surface(window, width, height, wgpu::PresentMode::AutoVsync).await?;
        SURFACE_FORMAT.get_or_init(|| surface.config.format);
        install_error_handler(&self.inner.devices[surface.dev_id].device);
        Ok(RenderSurface(surface))
    }

    pub(crate) fn resize_surface(&self, surface: &mut RenderSurface<'_>, width: u32, height: u32) {
        let device = &self.inner.devices[surface.0.dev_id].device;
        surface.0.config.width = width.max(1);
        surface.0.config.height = height.max(1);
        surface.0.surface.configure(device, &surface.0.config);
    }

    pub(crate) fn device_count(&self) -> usize {
        self.inner.devices.len()
    }

    pub(crate) fn new_renderer(&self, device: usize) -> Option<Renderer> {
        let dh = self.inner.devices.get(device)?;
        Some(new_renderer(&dh.device, &dh.queue))
    }

    /// Fatal uncaptured errors landed since this context was built (08 §6.5 step 1).
    pub(crate) fn needs_reset(&self) -> bool {
        GPU_ERRORS.load(std::sync::atomic::Ordering::Relaxed) != self.errors_seen
    }

    /// The hybrid host has no loss callback; its devices report through `needs_reset` only.
    pub(crate) fn lost_devices(&self) -> Vec<String> {
        Vec::new()
    }

    /// Soak tool (`PEBBLES_SYNTH_GPU_LOSS`): poison the stack as a fatal error would.
    pub(crate) fn inject_synthetic_loss(&self) {
        GPU_ERRORS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Deref for RenderContext {
    type Target = VelloCtx;
    fn deref(&self) -> &VelloCtx {
        &self.inner
    }
}

impl std::ops::DerefMut for RenderContext {
    fn deref_mut(&mut self) -> &mut VelloCtx {
        &mut self.inner
    }
}

/// Thin wrapper over `vello::util::RenderSurface`.
pub(crate) struct RenderSurface<'w>(vello::util::RenderSurface<'w>);

impl RenderSurface<'_> {
    pub(crate) fn device_index(&self) -> usize {
        self.0.dev_id
    }

    pub(crate) fn size(&self) -> (u32, u32) {
        (self.0.config.width, self.0.config.height)
    }
}

/// The hybrid backend has no renderer counters (06 §3.5 seam; unconditional at X02).
pub(crate) fn frame_stats_line(_renderer: &Renderer) -> Option<String> {
    None
}

/// The hybrid renderer: the `vello_hybrid::Renderer` + its `Resources`, built once.
pub(crate) struct Renderer {
    inner: Option<HybRenderer>,
    resources: Option<Resources>,
    image_cache: HashMap<u64, wgpu::Texture>,
}

pub(crate) fn new_renderer(_device: &wgpu::Device, _queue: &wgpu::Queue) -> Renderer {
    Renderer { inner: None, resources: None, image_cache: HashMap::new() }
}

impl Renderer {
    pub(crate) fn render_to_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        target: &wgpu::TextureView,
        params: &RenderParams,
    ) -> Result<(), String> {
        let format = *SURFACE_FORMAT.get().ok_or("pebbles(vello-hybrid): surface format not yet known")?;
        self.render_offscreen(
            device,
            queue,
            scene,
            target,
            params.base_color,
            (params.width, params.height),
            format,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_offscreen(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        target: &wgpu::TextureView,
        base_color: Color,
        size: (u32, u32),
        format: wgpu::TextureFormat,
    ) -> Result<(), String> {
        let size = (size.0.max(1), size.1.max(1));
        if self.inner.is_none() {
            let (renderer, resources) =
                HybRenderer::new(device, &RenderTargetConfig { format, width: size.0, height: size.1 });
            self.inner = Some(renderer);
            self.resources = Some(resources);
        }
        let renderer = self.inner.as_mut().expect("hybrid renderer built just above when inner was None");
        let resources =
            self.resources.as_mut().expect("hybrid resources built just above when inner was None");

        let mut hyb = HybScene::new(size.0.min(u16::MAX as u32) as u16, size.1.min(u16::MAX as u32) as u16);
        hyb.set_paint(base_color);
        hyb.fill_rect(&kurbo::Rect::new(0.0, 0.0, f64::from(size.0), f64::from(size.1)));
        scene.flush(&mut hyb, resources);

        let uploads = scene.image_uploads();
        let mut bindings = TextureBindings::new();
        for (i, up) in uploads.iter().enumerate() {
            let cache = &mut self.image_cache;
            let texture = cache.entry(up.id).or_insert_with(|| {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("pebbles.hybrid.image"),
                    size: wgpu::Extent3d {
                        width: up.width.max(1),
                        height: up.height.max(1),
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &up.rgba8,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(up.width * 4),
                        rows_per_image: Some(up.height),
                    },
                    wgpu::Extent3d { width: up.width, height: up.height, depth_or_array_layers: 1 },
                );
                texture
            });
            bindings
                .insert(TextureId(i as u64), texture.create_view(&wgpu::TextureViewDescriptor::default()));
        }

        let mut encoder = device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("pebbles.hybrid.render") });
        renderer
            .render(
                &hyb,
                resources,
                device,
                queue,
                &mut encoder,
                &RenderSize { width: size.0, height: size.1 },
                target,
                &bindings,
            )
            .map_err(|e| format!("vello_hybrid render: {e:?}"))?;
        queue.submit([encoder.finish()]);
        Ok(())
    }
}

/// Acquire the swapchain image, render `scene` straight into it, and present.
pub(crate) fn present_frame(
    ctx: &RenderContext,
    renderer: &mut Renderer,
    surface: &mut RenderSurface<'_>,
    scene: &Scene,
    base_color: Color,
    window: &winit::window::Window,
) -> PresentOutcome {
    let device = &ctx.inner.devices[surface.0.dev_id].device;
    let queue = &ctx.inner.devices[surface.0.dev_id].queue;
    let surface_texture = match surface.0.surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
        _ => return PresentOutcome::Skipped,
    };
    let view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
    let (w, h) = surface.size();
    let params = RenderParams { base_color, width: w, height: h, antialiasing_method: AaConfig::Area };
    if let Err(e) = renderer.render_to_texture(device, queue, scene, &view, &params) {
        eprintln!("pebbles: hybrid render failed (skipping frame, scheduling GPU reset): {e}");
        GPU_ERRORS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        window.request_redraw();
        return PresentOutcome::Skipped;
    }
    let _ = device.poll(wgpu::PollType::Poll);
    window.pre_present_notify();
    surface_texture.present();
    PresentOutcome::Presented
}

/// Global count of uncaptured GPU errors — the render loop compares it per frame and
/// rebuilds the poisoned GPU state when it moved (see [`install_error_handler`]).
static GPU_ERRORS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A GUI shell must not die on a GPU hiccup. wgpu's default uncaptured-error handler
/// panics; this one logs (throttled) and only bumps [`GPU_ERRORS`] for fatal errors.
/// Hybrid host only (C16): pebbles-gpu devices install their own hooks (`DeviceHealth`, C18), and
/// the "benign atlas" filter below is a Vello quirk workaround (deleted with the hybrid host, X01).
fn install_error_handler(device: &wgpu::Device) {
    use std::sync::atomic::AtomicU64;
    static VALIDATION_SEEN: AtomicU64 = AtomicU64::new(0);
    device.on_uncaptured_error(std::sync::Arc::new(|e: wgpu::Error| {
        let fatal = !matches!(e, wgpu::Error::Validation { .. });
        if fatal {
            let n = GPU_ERRORS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            pebbles_core::log::error(
                pebbles_core::log::Cat::Gpu,
                format!("FATAL wgpu error #{n} (device poisoned — scheduling reset): {e}"),
            );
            eprintln!("pebbles: fatal wgpu error #{n} — rebuilding the GPU state: {e}");
        } else {
            let n = VALIDATION_SEEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
            let text = e.to_string();
            let benign_atlas =
                text.contains("Validation") && (text.contains("create_view") || text.contains("Texture"));
            if benign_atlas {
                if n == 1 {
                    pebbles_core::log::warn(
                        pebbles_core::log::Cat::Gpu,
                        "vello glyph-atlas validation warning from the GPU driver (vello 0.10 on \
                         this Vulkan driver) — text still renders correctly; this is a driver/vello \
                         quirk, not a Pebbles error. Suppressing further repeats."
                            .to_string(),
                    );
                }
            } else if n <= 3 || n.is_multiple_of(600) {
                pebbles_core::log::warn(
                    pebbles_core::log::Cat::Gpu,
                    format!("non-fatal wgpu validation error (x{n}, ignored): {e}"),
                );
            }
        }
        if std::env::var("PEBBLES_GPU_TRACE").is_ok() {
            eprintln!("{}", std::backtrace::Backtrace::force_capture());
        }
    }));
}
