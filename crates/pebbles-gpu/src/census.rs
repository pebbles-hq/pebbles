//! `GpuCensus` (10 §9.2, M05): debug-only, count-only cache sizes for soak tests.

/// Debug-only, count-only. Soak tests compare it across rounds; leaks are judged by census,
/// never RSS (performance-standards §4).
///
/// `Renderer::census` fills the per-device fields and the `frame_target_*` fields (from its
/// book of rendered targets, M13); the `layer_pool_*` fields belong to one render target and are
/// filled by the owner of its `FrameTargets` (`OffscreenRenderer::census`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct GpuCensus {
    /// Resident + empty glyph and path-mask entries (both kinds).
    pub glyph_entries: usize,
    /// Mask atlas page side.
    pub glyph_mask_side: u32,
    /// Mask atlas physical layers.
    pub glyph_mask_layers: u32,
    /// Color atlas page side.
    pub glyph_color_side: u32,
    /// Color atlas physical layers.
    pub glyph_color_layers: u32,
    /// `RasterFont` cache entries (§5.11).
    pub raster_fonts: usize,
    /// Cached COLR glyph fragments (R29).
    pub color_glyph_fragments: usize,
    /// Cached images.
    pub image_entries: usize,
    /// Cached image texture bytes.
    pub image_bytes: u64,
    /// Image bind groups (one per image and sampler used with it).
    pub image_bind_groups: usize,
    /// Cached samplers.
    pub samplers: usize,
    /// Gradient LUT rows in use (0 until M15).
    pub gradient_rows_used: usize,
    /// Gradient LUT row capacity (0 until M15).
    pub gradient_rows_capacity: u32,
    /// Resident Tier B path and clip masks in the coverage atlas (M28).
    pub path_mask_entries: usize,
    /// Their atlas bytes (≤ `GpuBudgets::path_mask_bytes` when a frame fits, M28).
    pub path_mask_bytes: u64,
    /// `FrameTargets` owned (one per render target).
    pub frame_target_entries: usize,
    /// Bytes of target-owned textures.
    pub frame_target_bytes: u64,
    /// Pooled layer targets.
    pub layer_pool_textures: usize,
    /// Pooled layer bytes.
    pub layer_pool_bytes: u64,
    /// Pooled GPU clip-mask textures (09 §2.9).
    pub gpu_mask_textures: usize,
    /// Pooled GPU clip-mask bytes.
    pub gpu_mask_bytes: u64,
    /// Created render pipelines.
    pub pipelines: usize,
    /// Σ capacity of all frame arenas.
    pub arena_capacity_bytes: u64,
}
