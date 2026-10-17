//! Device-derived hard caps for content-driven work (11 §2.5). Plain data, no globals (DR-12).

/// Ceiling for any single GPU buffer, whatever the device allows.
pub const MAX_BUFFER_BYTES_CAP: u64 = 64 * 1024 * 1024;
/// Fixed by 10 §8.2 (changing it needs a DR). R20's ppem gate in prepare (02 §7.5) imports it;
/// the atlas (M01) refuses nothing by size.
pub const MAX_ATLAS_GLYPH_PX: u32 = 256;
/// Live offscreen layer targets (S12).
pub const MAX_LAYER_TARGETS: u32 = 16;
/// Nested layers (S12).
pub const MAX_LAYER_DEPTH: u32 = 32;
/// Device coordinates beyond this record `coordinate-out-of-range` (S02).
pub const MAX_COORD_PX: f32 = 4_194_304.0; // 2^22

/// Hard caps derived once from the device's granted limits (11 §2.5). Byte budgets live in 10 §8.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RendererLimits {
    /// `max_texture_dimension_2d` of the device.
    pub max_texture_dim: u32,
    /// `min(max_buffer_size, MAX_BUFFER_BYTES_CAP)`.
    pub max_buffer_bytes: u64,
    /// `min(MAX_ATLAS_GLYPH_PX, max_texture_dim / 4)` (informational).
    pub max_atlas_glyph_px: u32,
    /// Live offscreen layer targets.
    pub max_layer_targets: u32,
    /// Nested layer depth.
    pub max_layer_depth: u32,
    /// Largest device coordinate magnitude prepare expects.
    pub max_coord_px: f32,
}

impl RendererLimits {
    /// Build from the limits the DEVICE was granted (`device.limits()`), never the adapter's.
    pub fn from_device_limits(l: &wgpu::Limits) -> Self {
        Self {
            max_texture_dim: l.max_texture_dimension_2d,
            max_buffer_bytes: l.max_buffer_size.min(MAX_BUFFER_BYTES_CAP),
            max_atlas_glyph_px: MAX_ATLAS_GLYPH_PX.min(l.max_texture_dimension_2d / 4),
            max_layer_targets: MAX_LAYER_TARGETS,
            max_layer_depth: MAX_LAYER_DEPTH,
            max_coord_px: MAX_COORD_PX,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webgl2_caps() {
        let l = RendererLimits::from_device_limits(&wgpu::Limits::downlevel_webgl2_defaults());
        assert_eq!(l.max_texture_dim, 2048);
        assert_eq!(l.max_atlas_glyph_px, 256);
        assert_eq!(l.max_buffer_bytes, 64 * 1024 * 1024);
    }

    #[test]
    fn default_caps() {
        let l = RendererLimits::from_device_limits(&wgpu::Limits::default());
        assert_eq!(l.max_texture_dim, 8192);
        assert_eq!(l.max_buffer_bytes, 64 * 1024 * 1024);
    }
}
