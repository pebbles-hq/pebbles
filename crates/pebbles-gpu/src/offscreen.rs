//! The public headless renderer (06 §4.7): render a scene to unpremultiplied RGBA8 with no
//! window. Native only — the web has no blocking readback.

use std::time::Duration;

use pebbles_render::paint::Scene;

use crate::context::{DeviceId, GpuContext};
use crate::error::CaptureError;
use crate::frame_targets::{FrameParams, FrameTargets, RenderTarget};
use crate::init::GpuInit;
use crate::readback::{read_texture_rgba, unpremultiply_in_place};
use crate::renderer::Renderer;
use crate::stats::FrameStats;

/// How long a capture waits for the GPU before returning `Err`.
const READBACK_TIMEOUT: Duration = Duration::from_secs(10);

/// Tightly packed, row-major, UNPREMULTIPLIED, sRGB-encoded RGBA8 (`width * height * 4` bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct RgbaImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// The pixels.
    pub pixels: Vec<u8>,
}

/// Headless renderer: its own context, one device, one [`Renderer`], one [`FrameTargets`].
///
/// ```no_run
/// use pebbles_gpu::OffscreenRenderer;
/// use pebbles_render::paint::{Painter, Scene};
///
/// let mut offscreen = OffscreenRenderer::new()?;
/// let mut scene = Scene::new();
/// Painter::new(&mut scene).fill(
///     peniko::Fill::NonZero,
///     kurbo::Affine::IDENTITY,
///     peniko::Color::BLACK,
///     None,
///     &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
/// );
/// let image = offscreen.render_rgba(&scene, (64, 64), peniko::Color::WHITE, 1.0)?;
/// assert_eq!(image.pixels.len(), 64 * 64 * 4);
/// # Ok::<(), pebbles_gpu::CaptureError>(())
/// ```
pub struct OffscreenRenderer {
    context: GpuContext,
    device: DeviceId,
    renderer: Renderer,
    targets: FrameTargets,
    last_stats: FrameStats,
    /// `set_glyph_hint_inputs`: (scale-factor override, hint override).
    hint_inputs: (Option<f64>, Option<bool>),
    /// Test-only: issue a bad `write_texture` inside the error scopes.
    #[cfg(test)]
    inject_validation_error: bool,
}

impl std::fmt::Debug for OffscreenRenderer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OffscreenRenderer")
            .field("adapter", &self.adapter_info().name)
            .finish_non_exhaustive()
    }
}

impl OffscreenRenderer {
    /// Hard per-side cap (equals `pebbles_shell::capture::MAX_CAPTURE_DIM`, DR-16).
    pub const MAX_DIM: u32 = 16_384;

    /// `with_init(GpuInit::headless())`. Blocks (pollster).
    pub fn new() -> Result<Self, CaptureError> {
        Self::with_init(GpuInit::headless())
    }

    /// A headless renderer on the best adapter for `init`. Blocks (pollster).
    pub fn with_init(init: GpuInit) -> Result<Self, CaptureError> {
        let mut context = pollster::block_on(GpuContext::new(&init));
        let device = pollster::block_on(context.request_device(None)).map_err(CaptureError::Gpu)?;
        let gpu = context.device(device).ok_or(CaptureError::Gpu(crate::GpuError::DeviceLost))?;
        let renderer = Renderer::new(gpu, device);
        Ok(Self {
            context,
            device,
            renderer,
            targets: FrameTargets::new(),
            last_stats: FrameStats::default(),
            hint_inputs: (None, None),
            #[cfg(test)]
            inject_validation_error: false,
        })
    }

    fn gpu(&self) -> &crate::GpuDevice {
        // The device id came from this context and the context is never rebuilt.
        match self.context.device(self.device) {
            Some(gpu) => gpu,
            None => unreachable_device(),
        }
    }

    /// Render `scene` into `target` (an `Rgba8Unorm` view of `size`) without a readback — the
    /// bench's render-only entry (10 §11.2). Returns the submission to wait on.
    pub fn render_only(
        &mut self,
        scene: &Scene,
        target: &wgpu::TextureView,
        base_color: peniko::Color,
    ) -> Result<wgpu::SubmissionIndex, CaptureError> {
        let rt = RenderTarget::new(target, wgpu::TextureFormat::Rgba8Unorm, &mut self.targets);
        self.last_stats =
            self.renderer.render(rt, scene, &FrameParams::new(base_color)).map_err(CaptureError::Gpu)?;
        // An empty submit orders after the frame's: waiting on it waits for the frame.
        Ok(self.queue().submit([]))
    }

    /// Replace the renderer with a fresh one on the same device: every cache cold (the bench's
    /// `glyph_cold`, 10 §11.1).
    pub fn reset_renderer(&mut self) {
        self.renderer = Renderer::new(self.gpu(), self.device);
        self.targets = FrameTargets::new();
    }

    /// `min(MAX_DIM, RendererLimits::max_texture_dim)`.
    pub fn max_size(&self) -> u32 {
        Self::MAX_DIM.min(self.renderer.limits().max_texture_dim)
    }

    /// The adapter this renderer runs on.
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        &self.gpu().caps().info
    }

    /// The device, for tools that render next to it on the same device.
    pub fn device(&self) -> &wgpu::Device {
        self.gpu().device()
    }

    /// The queue.
    pub fn queue(&self) -> &wgpu::Queue {
        self.gpu().queue()
    }

    /// The `FrameStats` of the last `render_rgba`.
    /// Debug switch (R10): draw Tier A shapes through tessellation instead, as before R10
    /// (render-check `--tier-a off` measures the pre-card error).
    pub fn set_sdf_enabled(&mut self, enabled: bool) {
        self.renderer.set_sdf_enabled(enabled);
    }

    /// Hinting inputs for scenes already scaled to device px (render-check): `scale_factor`
    /// replaces the render scale as the 02 §7.8 device-pixel ratio, `hint` forces hinting on/off.
    /// `None` restores the defaults (R27).
    pub fn set_glyph_hint_inputs(&mut self, scale_factor: Option<f64>, hint: Option<bool>) {
        self.hint_inputs = (scale_factor, hint);
    }

    /// Debug-only, count-only cache sizes (10 §9.2, M05): the renderer's plus this target's.
    #[cfg(debug_assertions)]
    pub fn census(&self) -> crate::census::GpuCensus {
        let mut c = self.renderer.census();
        self.targets.census_into(&mut c);
        c
    }

    /// The `FrameStats` of the last render.
    pub fn last_frame_stats(&self) -> FrameStats {
        self.last_stats
    }

    /// Render `scene` scaled by `scale` into a `size` (device px) `Rgba8Unorm` texture cleared
    /// to `base_color`, read it back and unpremultiply. A size of 0 or above `max_size()` is
    /// `InvalidSize`. Render and copy run inside Validation, OutOfMemory and Internal error
    /// scopes (popped LIFO), so a GPU error returns `Err` instead of panicking.
    pub fn render_rgba(
        &mut self,
        scene: &Scene,
        size: (u32, u32),
        base_color: peniko::Color,
        scale: f64,
    ) -> Result<RgbaImage, CaptureError> {
        // C18: a lost device renders nothing — report it before creating any resource on it.
        if self.gpu().health().is_lost() {
            return Err(CaptureError::Gpu(crate::GpuError::DeviceLost));
        }
        let (width, height) = size;
        let max = self.max_size();
        if width == 0 || height == 0 || width > max || height > max {
            return Err(CaptureError::InvalidSize { width, height, max });
        }
        let device = self.device().clone();
        let queue = self.queue().clone();
        let oom = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let internal = device.push_error_scope(wgpu::ErrorFilter::Internal);
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pebbles.gpu.readback.texture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let target = RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, &mut self.targets);
        let params = FrameParams::new(base_color)
            .with_root_transform(kurbo::Affine::scale(scale))
            .with_scale_factor(self.hint_inputs.0.unwrap_or(scale))
            .with_glyph_hint(self.hint_inputs.1);
        let rendered = self.renderer.render(target, scene, &params);
        #[cfg(test)]
        if self.inject_validation_error {
            // Bytes for a region outside the texture: a validation error inside the scopes.
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: width, y: 0, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &[0; 4],
                wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4), rows_per_image: None },
                wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
            );
        }
        let pixels = rendered.map_err(CaptureError::Gpu).and_then(|stats| {
            self.last_stats = stats;
            read_texture_rgba(&device, &queue, &texture, READBACK_TIMEOUT)
        });

        // Pop LIFO (Validation, Internal, OutOfMemory); the first error wins.
        let errors = [
            pollster::block_on(validation.pop()),
            pollster::block_on(internal.pop()),
            pollster::block_on(oom.pop()),
        ];
        if let Some(e) = errors.into_iter().flatten().next() {
            return Err(CaptureError::Wgpu(e));
        }
        let mut pixels = pixels?;
        unpremultiply_in_place(&mut pixels);
        Ok(RgbaImage { width, height, pixels })
    }
}

#[cold]
#[cfg_attr(
    not(test),
    expect(clippy::panic, reason = "INVARIANT: a DeviceId minted by this context always resolves in it")
)]
fn unreachable_device() -> ! {
    panic!("OffscreenRenderer: its own device id no longer resolves")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "gpu"]
    fn capture_does_not_panic_on_validation_error() {
        let mut init = GpuInit::headless();
        init.strict_validation = false;
        let Ok(mut offscreen) = OffscreenRenderer::with_init(init) else {
            assert!(!std::env::var("PEBBLES_REQUIRE_GPU").is_ok_and(|v| v == "1"), "no GPU adapter");
            return;
        };
        offscreen.inject_validation_error = true;
        let result = offscreen.render_rgba(&Scene::new(), (16, 16), peniko::Color::WHITE, 1.0);
        assert!(matches!(result, Err(CaptureError::Wgpu(_))), "{result:?}");
    }
}
