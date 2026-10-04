//! A glyph atlas with a simple shelf packer: `R8Unorm` for coverage glyphs and
//! `Rgba8Unorm` for color (emoji) glyphs. Glyphs are cached by `CacheKey` across
//! frames; the texture is fixed-size for v1 (growth/eviction is a follow-up).

/// The glyph-atlas size (square). Fixed for v1.
pub(crate) const ATLAS_SIZE: u32 = 1024;

pub(crate) struct Atlas {
    pub(crate) texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    pub(crate) sampler: wgpu::Sampler,
    size: u32,
    cursor_x: u32,
    cursor_y: u32,
    row_height: u32,
}

pub(crate) struct AtlasRegion {
    pub x: u32,
    pub y: u32,
}

impl Atlas {
    pub(crate) fn new(device: &wgpu::Device, size: u32, format: wgpu::TextureFormat) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pebbles.gpu.atlas.mask"),
            size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("pebbles.gpu.atlas.sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self { texture, view, sampler, size, cursor_x: 1, cursor_y: 1, row_height: 0 }
    }

    /// Pack a `w`×`h` glyph into the atlas (with 1px padding), returning its region.
    /// Returns `None` when the atlas is full (the glyph is skipped for v1).
    pub(crate) fn pack(&mut self, w: u32, h: u32) -> Option<AtlasRegion> {
        let (w_pad, h_pad) = (w + 2, h + 2);
        if self.cursor_x + w_pad > self.size {
            self.cursor_x = 1;
            self.cursor_y += self.row_height;
            self.row_height = 0;
        }
        if self.cursor_y + h_pad > self.size {
            return None;
        }
        let region = AtlasRegion { x: self.cursor_x, y: self.cursor_y };
        self.cursor_x += w_pad;
        self.row_height = self.row_height.max(h_pad);
        Some(region)
    }
}
