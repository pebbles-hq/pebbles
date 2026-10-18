//! Per-target textures (10 §4): the MSAA color attachment of ONE render target, owned by its
//! `RenderSurface` / offscreen renderer — never by the per-device `Renderer` — so windows of
//! different sizes never thrash and a closed window frees its targets.

use crate::stats::FrameStats;

/// Identity of a target's textures; they are recreated only when it changes (E1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TargetKey {
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
    pub sample_count: u32,
}

/// Target-sized textures of one render target.
#[derive(Default)]
pub struct FrameTargets {
    key: Option<TargetKey>,
    msaa: Option<(wgpu::Texture, wgpu::TextureView)>,
}

impl std::fmt::Debug for FrameTargets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameTargets").field("key", &self.key).finish_non_exhaustive()
    }
}

impl FrameTargets {
    /// Empty targets; textures are created on the first render.
    pub fn new() -> Self {
        Self::default()
    }

    /// The MSAA view for `key`, (re)creating it when the key changed. `None` when single-sampled.
    pub(crate) fn msaa_view(
        &mut self,
        device: &wgpu::Device,
        key: TargetKey,
        stats: &mut FrameStats,
    ) -> Option<&wgpu::TextureView> {
        if key.sample_count <= 1 {
            self.key = Some(key);
            self.msaa = None;
            return None;
        }
        if self.key != Some(key) || self.msaa.is_none() {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("pebbles.gpu.target.msaa"),
                size: wgpu::Extent3d { width: key.width, height: key.height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: key.sample_count,
                dimension: wgpu::TextureDimension::D2,
                format: key.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            stats.textures_created += 1;
            stats.texture_views_created += 1;
            self.msaa = Some((texture, view));
            self.key = Some(key);
        }
        self.msaa.as_ref().map(|m| &m.1)
    }
}

/// Where one frame renders: a view, its format, and the target's own textures.
pub struct RenderTarget<'a> {
    pub(crate) view: &'a wgpu::TextureView,
    format: wgpu::TextureFormat,
    size: (u32, u32),
    pub(crate) targets: &'a mut FrameTargets,
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
        Self { view, format, size: (texture.width(), texture.height()), targets }
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
}

impl FrameParams {
    /// Clear to `base_color`, identity root transform.
    pub fn new(base_color: peniko::Color) -> Self {
        Self { base_color, root_transform: kurbo::Affine::IDENTITY }
    }

    /// Replace the root transform.
    #[must_use]
    pub fn with_root_transform(mut self, root_transform: kurbo::Affine) -> Self {
        self.root_transform = root_transform;
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
