//! Shared bind-group layouts, created once per Renderer (06 §9.4).

/// The two bind-group layouts every pipeline uses: group 0 = Globals, group 1 = a
/// `texture_2d` + sampler (glyph atlases and images share it).
pub(crate) struct BindGroupLayouts {
    pub globals: wgpu::BindGroupLayout,
    pub texture: wgpu::BindGroupLayout,
    /// Glyph group 1: mask array, color array, filtering sampler (06 §9.4).
    pub glyph: wgpu::BindGroupLayout,
    /// Composite group 1: the layer texture (non-filterable, `textureLoad`) + `CompositeParams`
    /// (uniform, dynamic offset) (09 §3.7).
    pub composite: wgpu::BindGroupLayout,
    pub composite_backdrop: wgpu::BindGroupLayout,
    pub blit: wgpu::BindGroupLayout,
    pub gradient: wgpu::BindGroupLayout,
    pub image_brush: wgpu::BindGroupLayout,
    pub effect: wgpu::BindGroupLayout,
    pub projected: wgpu::BindGroupLayout,
    /// 09 §6.1: shader brush = t0, t1 (filterable), s0, `ShaderBrushParams` (dynamic, 304).
    pub shader_brush: wgpu::BindGroupLayout,
}

impl BindGroupLayouts {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let globals = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pebbles.gpu.layout.globals"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(super::globals::GLOBALS_SIZE),
                    },
                    count: None,
                },
                // 09 §2.4: the active SDF clip (`ClipParams`), dynamic offset.
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(32),
                    },
                    count: None,
                },
                // 09 L08: the active mask clip (`MaskClipParams`), dynamic offset.
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(32),
                    },
                    count: None,
                },
                // 09 L08: the coverage atlas mask pages (`textureLoad`, texel-exact).
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                // 09 L09: the active GPU coverage mask (a mask pool texture, or a 1×1 dummy).
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let texture = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pebbles.gpu.layout.texture"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // DR-21: the sampler's `SamplerKey::mode` (16 B); the shaders filter in code.
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                },
            ],
        });
        let array = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2Array,
                multisampled: false,
            },
            count: None,
        };
        let glyph = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pebbles.gpu.layout.glyph"),
            entries: &[
                array(0),
                array(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let composite = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pebbles.gpu.layout.composite"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(32),
                    },
                    count: None,
                },
            ],
        });
        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        // 09 §4.4: the backdrop composite adds the parent's copied pixels at binding 2.
        let composite_backdrop = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pebbles.gpu.layout.composite_backdrop"),
            entries: &[
                tex_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(32),
                    },
                    count: None,
                },
                tex_entry(2),
            ],
        });
        let blit = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pebbles.gpu.layout.blit"),
            entries: &[tex_entry(0)],
        });
        // 09 §5.4: the gradient LUT + `GradientParams` (dynamic, 80 bytes).
        let gradient = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pebbles.gpu.layout.gradient"),
            entries: &[
                tex_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(80),
                    },
                    count: None,
                },
            ],
        });
        // 09 §6: image brush = filterable texture + sampler + `ImageBrushParams` (dynamic, 48).
        let image_brush = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pebbles.gpu.layout.image_brush"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(48),
                    },
                    count: None,
                },
            ],
        });
        let tex_f = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let shader_brush = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pebbles.gpu.layout.shader_brush"),
            entries: &[
                tex_f(0),
                tex_f(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(304),
                    },
                    count: None,
                },
            ],
        });
        // 09 §10: an effect pass reads one texture (textureLoad) with `EffectParams` (224 B).
        let effect = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pebbles.gpu.layout.effect"),
            entries: &[
                tex_entry(0),
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(224),
                    },
                    count: None,
                },
            ],
        });
        // 09 §10.7: the projected layer = filterable texture + sampler + `ProjectedParams` (112 B).
        let projected = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("pebbles.gpu.layout.projected"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(112),
                    },
                    count: None,
                },
            ],
        });
        Self {
            globals,
            texture,
            glyph,
            composite,
            composite_backdrop,
            blit,
            gradient,
            image_brush,
            shader_brush,
            effect,
            projected,
        }
    }
}

/// Sampler identity (10 §3.5): ≤ 27 combinations by construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SamplerKey {
    /// mag = min = this (Low: Nearest; Medium, High: Linear — 02 §8.1).
    pub filter: wgpu::FilterMode,
    /// Nearest for Low and Medium, Linear for High (R35; one level until then).
    pub mipmap: wgpu::MipmapFilterMode,
    pub address_u: wgpu::AddressMode,
    pub address_v: wgpu::AddressMode,
}

impl Default for SamplerKey {
    fn default() -> Self {
        Self::of(peniko::ImageSampler::default())
    }
}

impl SamplerKey {
    /// The §3.5 mapping of a peniko image sampler.
    pub(crate) fn of(s: peniko::ImageSampler) -> Self {
        let address = |e: peniko::Extend| match e {
            peniko::Extend::Pad => wgpu::AddressMode::ClampToEdge,
            peniko::Extend::Repeat => wgpu::AddressMode::Repeat,
            peniko::Extend::Reflect => wgpu::AddressMode::MirrorRepeat,
        };
        let (filter, mipmap) = match s.quality {
            peniko::ImageQuality::Low => (wgpu::FilterMode::Nearest, wgpu::MipmapFilterMode::Nearest),
            peniko::ImageQuality::Medium => (wgpu::FilterMode::Linear, wgpu::MipmapFilterMode::Nearest),
            peniko::ImageQuality::High => (wgpu::FilterMode::Linear, wgpu::MipmapFilterMode::Linear),
        };
        Self { filter, mipmap, address_u: address(s.x_extend), address_v: address(s.y_extend) }
    }

    /// The bits `sample_portable` (prelude.wgsl) filters by: bit 0 linear, bit 1 linear
    /// between levels, bits 2–3 / 4–5 the u / v address mode (0 clamp, 1 repeat, 2 mirror).
    pub(crate) fn mode(self) -> u32 {
        let address = |a: wgpu::AddressMode| match a {
            wgpu::AddressMode::Repeat => 1,
            wgpu::AddressMode::MirrorRepeat => 2,
            _ => 0,
        };
        u32::from(self.filter == wgpu::FilterMode::Linear)
            | u32::from(self.mipmap == wgpu::MipmapFilterMode::Linear) << 1
            | address(self.address_u) << 2
            | address(self.address_v) << 4
    }
}

/// A 16-byte uniform holding `mode` (binding 2 of the texture layout).
pub(crate) fn mode_buffer(device: &wgpu::Device, mode: u32) -> wgpu::Buffer {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("pebbles.gpu.buffer.sampler_mode"),
        size: 16,
        usage: wgpu::BufferUsages::UNIFORM,
        mapped_at_creation: true,
    });
    buffer.slice(..).get_mapped_range_mut().copy_from_slice(bytemuck::bytes_of(&[mode, 0, 0, 0]));
    buffer.unmap();
    buffer
}

/// Samplers by key, created once per Renderer (10 §3.5).
#[derive(Default)]
pub(crate) struct SamplerCache {
    samplers: std::collections::HashMap<SamplerKey, wgpu::Sampler>,
    modes: std::collections::HashMap<SamplerKey, wgpu::Buffer>,
}

impl SamplerCache {
    /// Cached sampler count (census).
    #[cfg(debug_assertions)]
    pub(crate) fn len(&self) -> usize {
        self.samplers.len()
    }

    /// The sampler of `key` and its `SamplerKey::mode` uniform, created on first use.
    pub(crate) fn get_with_mode(
        &mut self,
        device: &wgpu::Device,
        key: SamplerKey,
        stats: &mut crate::stats::FrameStats,
    ) -> (&wgpu::Sampler, &wgpu::Buffer) {
        self.get_or_create(device, key, stats);
        if let std::collections::hash_map::Entry::Vacant(e) = self.modes.entry(key) {
            stats.buffers_created += 1;
            e.insert(mode_buffer(device, key.mode()));
        }
        (&self.samplers[&key], &self.modes[&key])
    }

    /// The sampler for `key`, created on first use (`stats.samplers_created`).
    pub(crate) fn get_or_create(
        &mut self,
        device: &wgpu::Device,
        key: SamplerKey,
        stats: &mut crate::stats::FrameStats,
    ) -> &wgpu::Sampler {
        self.samplers.entry(key).or_insert_with(|| {
            stats.samplers_created += 1;
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("pebbles.gpu.sampler"),
                address_mode_u: key.address_u,
                address_mode_v: key.address_v,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: key.filter,
                min_filter: key.filter,
                mipmap_filter: key.mipmap,
                ..Default::default()
            })
        })
    }
}

/// A bind group valid for one key (a resource generation); rebuilt only when the key changes.
pub(crate) struct GenBindGroup {
    pub key: u64,
    pub group: wgpu::BindGroup,
}

/// Rebuild `slot` only when `key` differs from the cached one (`stats.bind_groups_created`).
pub(crate) fn refresh<'a>(
    slot: &'a mut Option<GenBindGroup>,
    key: u64,
    create: impl FnOnce() -> wgpu::BindGroup,
    stats: &mut crate::stats::FrameStats,
) -> &'a wgpu::BindGroup {
    if slot.as_ref().is_some_and(|g| g.key != key) {
        *slot = None;
    }
    &slot
        .get_or_insert_with(|| {
            stats.bind_groups_created += 1;
            GenBindGroup { key, group: create() }
        })
        .group
}

#[cfg(test)]
mod sampler_tests {
    use super::SamplerKey;

    #[test]
    fn sampler_key_maps_quality_and_extend() {
        let s = peniko::ImageSampler {
            quality: peniko::ImageQuality::Low,
            x_extend: peniko::Extend::Repeat,
            y_extend: peniko::Extend::Reflect,
            ..peniko::ImageSampler::default()
        };
        let k = SamplerKey::of(s);
        assert_eq!(k.filter, wgpu::FilterMode::Nearest);
        assert_eq!((k.address_u, k.address_v), (wgpu::AddressMode::Repeat, wgpu::AddressMode::MirrorRepeat));
        let high = peniko::ImageSampler { quality: peniko::ImageQuality::High, ..s };
        assert_eq!(SamplerKey::of(high).mipmap, wgpu::MipmapFilterMode::Linear);
    }
}
