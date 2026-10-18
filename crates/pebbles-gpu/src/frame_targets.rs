//! Per-target textures (10 §4): the offscreen layer pool of ONE render target, owned by its
//! `RenderSurface` / offscreen renderer — never by the per-device `Renderer` — so windows of
//! different sizes never thrash and a closed window frees its targets.

/// Target-sized textures of one render target. The main pass is single-sampled (R17): the
/// target owns only its pool of offscreen Tier C / layer targets (09 §3.8).
#[derive(Default)]
pub struct FrameTargets {
    pub(crate) pool: crate::execute::resources::layer_pool::LayerPool,
}

impl std::fmt::Debug for FrameTargets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameTargets").field("pool_bytes", &self.pool.bytes()).finish_non_exhaustive()
    }
}

impl FrameTargets {
    /// Empty targets; textures are created on first use.
    pub fn new() -> Self {
        Self::default()
    }

    /// Census fields owned by one render target (M05).
    #[cfg(debug_assertions)]
    pub(crate) fn census_into(&self, c: &mut crate::census::GpuCensus) {
        let bytes = self.pool.bytes();
        c.frame_target_entries += 1;
        c.frame_target_bytes += bytes;
        c.layer_pool_textures += self.pool.len();
        c.layer_pool_bytes += bytes;
    }

    /// Bytes of GPU textures this target owns beyond its own color texture (census, 10 §9).
    pub fn frame_target_bytes(&self) -> u64 {
        self.pool.bytes()
    }
}

/// Where one frame renders: a view, its format, and the target's own textures.
pub struct RenderTarget<'a> {
    pub(crate) view: &'a wgpu::TextureView,
    format: wgpu::TextureFormat,
    size: (u32, u32),
    pub(crate) targets: &'a mut FrameTargets,
    /// GL sRGB-only surface: shaders decode sRGB before the hardware re-encodes (02 §3.8, R13).
    pub(crate) decode_srgb_output: bool,
}

impl<'a> RenderTarget<'a> {
    /// `format` is the VIEW format (it may differ from the texture format on the sRGB-view path).
    /// The size is read from `view.texture()` (DR-12: size from the target, never from a window).
    pub fn new(
        view: &'a wgpu::TextureView,
        format: wgpu::TextureFormat,
        targets: &'a mut FrameTargets,
    ) -> Self {
        let texture = view.texture();
        Self { view, format, size: (texture.width(), texture.height()), targets, decode_srgb_output: false }
    }

    /// Mark the target as a GL sRGB-only surface (02 §3.8; from
    /// [`RenderSurface::decode_srgb_output`](crate::RenderSurface::decode_srgb_output)). Never set
    /// it for offscreen Unorm targets.
    #[must_use]
    pub fn with_decode_srgb_output(mut self, on: bool) -> Self {
        self.decode_srgb_output = on;
        self
    }

    /// Size in device pixels, from the target texture.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// The view format the renderer draws in.
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }
}

/// Per-frame parameters.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct FrameParams {
    /// Clear color, sRGB-encoded (premultiplied by `prepare::color::clear_color`).
    pub base_color: peniko::Color,
    /// Applied to the whole scene (DPI scale in the end state; identity in the transition).
    pub root_transform: kurbo::Affine,
    /// The target's device-pixel ratio (02 §7.8 hinting policy; R27). Default 1.0.
    pub scale_factor: f64,
    /// Force glyph hinting on/off for experiments (render-check `--glyph-hint`); `None` = the
    /// 02 §7.8 policy.
    pub glyph_hint: Option<bool>,
}

impl FrameParams {
    /// Clear to `base_color`, identity root transform.
    pub fn new(base_color: peniko::Color) -> Self {
        Self { base_color, root_transform: kurbo::Affine::IDENTITY, scale_factor: 1.0, glyph_hint: None }
    }

    /// Replace the root transform.
    #[must_use]
    pub fn with_root_transform(mut self, root_transform: kurbo::Affine) -> Self {
        self.root_transform = root_transform;
        self
    }

    /// Set the device-pixel ratio (R27).
    #[must_use]
    pub fn with_scale_factor(mut self, scale_factor: f64) -> Self {
        self.scale_factor = scale_factor;
        self
    }

    /// Force glyph hinting on or off; `None` restores the policy (R27 experiments).
    #[must_use]
    pub fn with_glyph_hint(mut self, hint: Option<bool>) -> Self {
        self.glyph_hint = hint;
        self
    }
}

/// 4 when the color format supports 4× + resolve and the depth format (if the target
/// has one, R62) supports 4×; otherwise 1. Never 8 (DR-06; 02 §5.4).
pub(crate) fn negotiate_sample_count(
    color: wgpu::TextureFormatFeatureFlags,
    depth: Option<wgpu::TextureFormatFeatureFlags>,
) -> u32 {
    let color_ok = color.sample_count_supported(4)
        && color.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE);
    let depth_ok = depth.is_none_or(|f| f.sample_count_supported(4));
    if color_ok && depth_ok { 4 } else { 1 }
}

/// The negotiated count for `color` (+ optional `depth`) on `adapter` (02 §5.4).
pub(crate) fn sample_count_for(
    adapter: &wgpu::Adapter,
    color: wgpu::TextureFormat,
    depth: Option<wgpu::TextureFormat>,
) -> u32 {
    negotiate_sample_count(
        adapter.get_texture_format_features(color).flags,
        depth.map(|f| adapter.get_texture_format_features(f).flags),
    )
}

/// Per-device cache of [`sample_count_for`] decisions keyed by `(color, depth)` (10 §4.3), so
/// Tier C runs, GPU clip masks (`R8Unorm`, L09) and depth targets (R62) share one cache.
#[derive(Debug, Default)]
pub(crate) struct MsaaCache {
    decided: std::collections::HashMap<(wgpu::TextureFormat, Option<wgpu::TextureFormat>), u32>,
}

impl MsaaCache {
    /// The sample count for `(color, depth)`, logged on the first decision per pair.
    pub(crate) fn get(
        &mut self,
        adapter: &wgpu::Adapter,
        color: wgpu::TextureFormat,
        depth: Option<wgpu::TextureFormat>,
    ) -> u32 {
        *self.decided.entry((color, depth)).or_insert_with(|| {
            let n = sample_count_for(adapter, color, depth);
            log_decision(adapter, color, depth, n);
            n
        })
    }
}

fn log_decision(
    adapter: &wgpu::Adapter,
    color: wgpu::TextureFormat,
    depth: Option<wgpu::TextureFormat>,
    n: u32,
) {
    use pebbles_foundation::log::{Cat, Level, logf};
    if n == 1 {
        logf(
            Level::Warn,
            Cat::Gpu,
            format_args!("MSAA unavailable for {color:?}; rendering without multisampling"),
        );
        return;
    }
    let flags = adapter.get_texture_format_features(color).flags;
    let a = flags.sample_count_supported(4)
        && flags.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE);
    let b = depth.is_none_or(|d| adapter.get_texture_format_features(d).flags.sample_count_supported(4));
    logf(
        Level::Info,
        Cat::Gpu,
        format_args!("MSAA: {color:?} → {n}× (color 4×+resolve: {a}, depth 4×: {b})"),
    );
}

#[cfg(test)]
mod tests {
    use super::negotiate_sample_count;
    use wgpu::TextureFormatFeatureFlags as F;

    #[test]
    fn x4_with_resolve_and_depth() {
        assert_eq!(
            negotiate_sample_count(F::MULTISAMPLE_X4 | F::MULTISAMPLE_RESOLVE, Some(F::MULTISAMPLE_X4)),
            4
        );
    }

    #[test]
    fn x4_without_resolve() {
        assert_eq!(negotiate_sample_count(F::MULTISAMPLE_X4, None), 1);
    }

    #[test]
    fn depth_without_x4() {
        assert_eq!(negotiate_sample_count(F::MULTISAMPLE_X4 | F::MULTISAMPLE_RESOLVE, Some(F::empty())), 1);
    }

    #[test]
    fn no_depth() {
        assert_eq!(negotiate_sample_count(F::MULTISAMPLE_X4 | F::MULTISAMPLE_RESOLVE, None), 4);
    }

    #[test]
    fn only_x8() {
        assert_eq!(negotiate_sample_count(F::MULTISAMPLE_X8 | F::MULTISAMPLE_RESOLVE, None), 1);
    }
}
