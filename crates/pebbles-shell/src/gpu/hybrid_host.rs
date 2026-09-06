//! The Vello Hybrid GPU host. It reuses `vello::util` for the backend-agnostic parts —
//! the wgpu instance/adapter/device pool (`RenderContext`), the swapchain surface, and the
//! `TextureBlitter` that copies the intermediate target to the swapchain — and changes only
//! two things:
//!
//!   1. the intermediate target texture is `RENDER_ATTACHMENT | TEXTURE_BINDING` (Vello
//!      Hybrid rasterizes into a render attachment; Vello-compute wrote a storage texture),
//!   2. the [`Renderer`] flushes the recorded op-list into a `vello_hybrid::Scene` and
//!      rasterizes it with `vello_hybrid::Renderer` instead of `vello::Renderer`.
//!
//! Everything else in the runner's frame loop (surface creation, resize, the blit + present)
//! is unchanged. NOTE: this path is compiled but has NOT been run in a window yet — the
//! agent's sandbox is headless; the flush itself is GPU-verified in `spikes/renderer-bench`.

use std::ops::Deref;
use std::sync::OnceLock;

use std::collections::HashMap;
use vello::util::RenderContext as VelloCtx;
pub(crate) use vello::util::RenderSurface;

use vello_hybrid::{
    RenderSize, RenderTargetConfig, Renderer as HybRenderer, Resources, Scene as HybScene, TextureBindings,
    TextureId,
};

use pebbles_render::Scene;

/// The swapchain texture format chosen by `vello::util` (Rgba8Unorm / Bgra8Unorm). Captured
/// once at surface creation so the lazily-built `vello_hybrid::Renderer` targets the right one.
static SURFACE_FORMAT: OnceLock<wgpu::TextureFormat> = OnceLock::new();

/// Antialiasing selector — the hybrid backend antialiases internally, so this only exists to
/// mirror the Vello host's `RenderParams` shape (the runner names `AaConfig::Area`).
#[derive(Clone, Copy, Debug)]
pub(crate) enum AaConfig {
    Area,
}

/// Per-frame render parameters — same fields the runner fills for the Vello host.
pub(crate) struct RenderParams {
    pub base_color: peniko::Color,
    pub width: u32,
    pub height: u32,
    #[allow(dead_code)]
    pub antialiasing_method: AaConfig,
}

/// The wgpu context — a thin wrapper over `vello::util::RenderContext` that, after creating
/// or resizing a surface, swaps the intermediate target to a render-attachment texture the
/// hybrid rasterizer can draw into. `Deref` exposes the inner `devices` pool unchanged.
pub(crate) struct RenderContext(VelloCtx);

impl RenderContext {
    pub(crate) fn new() -> Self {
        Self(VelloCtx::new())
    }

    pub(crate) async fn create_surface<'w>(
        &mut self,
        window: impl Into<wgpu::SurfaceTarget<'w>>,
        width: u32,
        height: u32,
        present_mode: wgpu::PresentMode,
    ) -> Result<RenderSurface<'w>, vello::Error> {
        let mut surface = self.0.create_surface(window, width, height, present_mode).await?;
        let _ = SURFACE_FORMAT.set(surface.config.format);
        let device = &self.0.devices[surface.dev_id].device;
        swap_target(device, &mut surface, width, height);
        Ok(surface)
    }

    pub(crate) fn resize_surface(&self, surface: &mut RenderSurface<'_>, width: u32, height: u32) {
        self.0.resize_surface(surface, width, height);
        let device = &self.0.devices[surface.dev_id].device;
        swap_target(device, surface, width, height);
    }
}

impl Deref for RenderContext {
    type Target = VelloCtx;
    fn deref(&self) -> &VelloCtx {
        &self.0
    }
}

/// Replace the surface's intermediate target with a `RENDER_ATTACHMENT | TEXTURE_BINDING`
/// texture — hybrid rasterizes into it, and the (unchanged) blitter samples it to present.
fn swap_target(device: &wgpu::Device, surface: &mut RenderSurface<'_>, width: u32, height: u32) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.hybrid.target"),
        size: wgpu::Extent3d { width: width.max(1), height: height.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: surface.config.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    surface.target_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    surface.target_texture = texture;
}

/// The hybrid renderer: the `vello_hybrid::Renderer` + its `Resources`, built lazily once the
/// target size is known and rebuilt on resize. `render_to_texture` mirrors `vello::Renderer`'s
/// signature so the runner calls it identically.
pub(crate) struct Renderer {
    inner: Option<HybRenderer>,
    resources: Option<Resources>,
    size: (u32, u32),
    /// GPU textures for `draw_image` content, keyed by source-pixel identity so an image
    /// isn't re-uploaded every frame. (Grows with distinct images seen; a real eviction
    /// policy is a follow-up — fine for typical screens.)
    image_cache: HashMap<u64, wgpu::Texture>,
}

/// Construct an (empty) hybrid renderer; the inner GPU renderer is built on first frame, when
/// the target format + size are known (the runner has no size at `new_renderer` time).
pub(crate) fn new_renderer(_device: &wgpu::Device, _queue: &wgpu::Queue) -> Renderer {
    Renderer { inner: None, resources: None, size: (0, 0), image_cache: HashMap::new() }
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
        let size = (params.width.max(1), params.height.max(1));
        if self.inner.is_none() || self.size != size {
            let (renderer, resources) =
                HybRenderer::new(device, &RenderTargetConfig { format, width: size.0, height: size.1 });
            self.inner = Some(renderer);
            self.resources = Some(resources);
            self.size = size;
        }
        let renderer = self.inner.as_mut().unwrap();
        let resources = self.resources.as_mut().unwrap();

        // Build the backend scene: clear to the base color, then flush the recorded op-list.
        let mut hyb = HybScene::new(size.0.min(u16::MAX as u32) as u16, size.1.min(u16::MAX as u32) as u16);
        hyb.set_paint(params.base_color);
        hyb.fill_rect(&kurbo::Rect::new(0.0, 0.0, f64::from(size.0), f64::from(size.1)));
        scene.flush(&mut hyb, resources);

        // Bind the external textures the flush references (`draw_image` content). Upload order
        // matches the flush's `TextureId(index)`; textures are cached by source identity so an
        // unchanged image isn't re-uploaded next frame.
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
