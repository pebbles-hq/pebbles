//! The candidate renderer (§6.8): pebbles-gpu's offscreen API. The only file that names
//! C15 / C21 items.

use pebbles_gpu::{FrameStats, GpuInit, OffscreenRenderer, Unsupported};
use pebbles_render::paint::Scene;

use crate::CheckError;
use crate::metric::Rgba8;

/// `pebbles_gpu::OffscreenRenderer` on the adapter `GpuInit::from_env()` selects.
pub struct Candidate {
    r: OffscreenRenderer,
}

impl Candidate {
    /// A renderer on a fresh device. Failure means no usable adapter (exit 3).
    pub fn new() -> Result<Self, CheckError> {
        OffscreenRenderer::with_init(GpuInit::from_env())
            .map(|r| Self { r })
            .map_err(|e| CheckError::NoAdapter(e.to_string()))
    }

    /// The device (shared with the smoke reference).
    pub fn device(&self) -> &wgpu::Device {
        self.r.device()
    }

    /// The queue.
    pub fn queue(&self) -> &wgpu::Queue {
        self.r.queue()
    }

    /// The adapter.
    pub fn adapter_info(&self) -> &wgpu::AdapterInfo {
        self.r.adapter_info()
    }

    /// Render the logical `scene` at `scale` into `size` device px over an opaque `bg`.
    pub fn render_rgba(
        &mut self,
        scene: &Scene,
        size: (u32, u32),
        bg: peniko::Color,
        scale: f64,
    ) -> Result<Rgba8, CheckError> {
        let img = self
            .r
            .render_rgba(scene, size, bg.with_alpha(1.0), scale)
            .map_err(|e| CheckError::CandidateGpu(e.to_string()))?;
        Ok(Rgba8 { width: img.width, height: img.height, data: img.pixels })
    }

    /// Tier A on/off (debug: off reproduces the pre-R10 tessellated path).
    pub fn set_tier_a(&mut self, on: bool) {
        self.r.set_sdf_enabled(on);
    }

    /// The `FrameStats` of the last render.
    pub fn last_frame_stats(&self) -> FrameStats {
        self.r.last_frame_stats()
    }

    /// Non-zero `label=count` fallback pairs of the last render (report `fallbacks`).
    pub fn last_fallbacks(&self) -> Vec<(&'static str, u32)> {
        let stats = self.last_frame_stats();
        Unsupported::ALL
            .iter()
            .filter_map(|k| {
                let n = stats.unsupported[*k as usize];
                (n > 0).then(|| (k.label(), n))
            })
            .collect()
    }

    /// Replace the renderer with one on a new device (after a PANIC or GPU_ERROR).
    pub fn recreate(&mut self) -> Result<(), CheckError> {
        *self = Self::new()?;
        Ok(())
    }
}
