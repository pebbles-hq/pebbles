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
