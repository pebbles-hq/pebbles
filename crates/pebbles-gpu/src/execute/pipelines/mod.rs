//! Pipeline families, the pipeline cache and the once-per-Renderer shader modules.

use std::collections::{HashMap, HashSet};

use super::resources::bind_groups::BindGroupLayouts;
use crate::stats::FrameStats;

mod composite;
mod glyph;
mod image;
mod sdf;
mod shadow;
mod solid;

/// One render pipeline family. Each maps to exactly one `.wgsl` file (§9.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[allow(dead_code)] // later cards (R10, L30, L13, …) add their pipelines
pub(crate) enum PipelineKind {
    Solid,
    Sdf,
    Shadow,
    Glyph,
    Image,
    Gradient,
    CompositeOver,
    CompositeCompose,
    CompositeBackdrop,
    Blit,
    Vertices,
    Sprite,
    Blur,
    ColorMatrix,
    Morphology,
    Projected,
    ShaderBrush,
    MaskMultiply,
}

impl PipelineKind {
    /// `pebbles.gpu.draw.<kind>` debug-group label (11 §4.4); static, never allocated per frame.
    pub(crate) const fn debug_label(self) -> &'static str {
        match self {
            Self::Solid => "pebbles.gpu.draw.solid",
            Self::Sdf => "pebbles.gpu.draw.sdf",
            Self::Shadow => "pebbles.gpu.draw.shadow",
            Self::Glyph => "pebbles.gpu.draw.glyph",
            Self::Image => "pebbles.gpu.draw.image",
            Self::Gradient => "pebbles.gpu.draw.gradient",
            Self::CompositeOver => "pebbles.gpu.draw.composite_over",
            Self::CompositeCompose => "pebbles.gpu.draw.composite_compose",
            Self::CompositeBackdrop => "pebbles.gpu.draw.composite_backdrop",
            Self::Blit => "pebbles.gpu.draw.blit",
            Self::Vertices => "pebbles.gpu.draw.vertices",
            Self::Sprite => "pebbles.gpu.draw.sprite",
            Self::Blur => "pebbles.gpu.draw.blur",
            Self::ColorMatrix => "pebbles.gpu.draw.color_matrix",
            Self::Morphology => "pebbles.gpu.draw.morphology",
            Self::Projected => "pebbles.gpu.draw.projected",
            Self::ShaderBrush => "pebbles.gpu.draw.shader_brush",
            Self::MaskMultiply => "pebbles.gpu.draw.mask_multiply",
        }
    }
}

/// Identity of one render pipeline (10 §3.2). No depth-stencil state (DR-20).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PipelineKey {
    pub kind: PipelineKind,
    pub color_format: wgpu::TextureFormat,
    pub sample_count: u32,
    pub blend: u8,
    pub variant: u8,
}

/// The once-per-Renderer shader modules (prelude prepended to each).
pub(crate) struct ShaderModules {
    pub solid: wgpu::ShaderModule,
    pub glyph: wgpu::ShaderModule,
    pub image: wgpu::ShaderModule,
    pub shadow: wgpu::ShaderModule,
    pub sdf: wgpu::ShaderModule,
    pub composite: wgpu::ShaderModule,
}

impl ShaderModules {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let module = |label: &str, source: &'static str| {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some(label),
                source: wgpu::ShaderSource::Wgsl(source.into()),
            })
        };
        use crate::shaders;
        Self {
            solid: module("pebbles.gpu.shader.solid", shaders::SOLID),
            glyph: module("pebbles.gpu.shader.glyph", shaders::GLYPH),
            image: module("pebbles.gpu.shader.image", shaders::IMAGE),
            shadow: module("pebbles.gpu.shader.shadow", shaders::SHADOW),
            sdf: module("pebbles.gpu.shader.sdf", shaders::SDF),
            composite: module("pebbles.gpu.shader.composite", shaders::COMPOSITE),
        }
    }

    pub(crate) fn for_kind(&self, kind: PipelineKind) -> &wgpu::ShaderModule {
        match kind {
            PipelineKind::Solid => &self.solid,
            PipelineKind::Glyph => &self.glyph,
            PipelineKind::Image => &self.image,
            PipelineKind::Shadow => &self.shadow,
            PipelineKind::Sdf => &self.sdf,
            PipelineKind::CompositeOver => &self.composite,
            _ => &self.solid,
        }
    }
}

/// Caches render pipelines and per-kind pipeline layouts (10 §3.2).
pub(crate) struct PipelineCache {
    layouts: HashMap<PipelineKind, wgpu::PipelineLayout>,
    pipelines: HashMap<PipelineKey, wgpu::RenderPipeline>,
    prewarmed: HashSet<(wgpu::TextureFormat, u32)>,
}

impl PipelineCache {
    pub(crate) fn new() -> Self {
        Self { layouts: HashMap::new(), pipelines: HashMap::new(), prewarmed: HashSet::new() }
    }

    fn layout_for(
        &mut self,
        device: &wgpu::Device,
        kind: PipelineKind,
        layouts: &BindGroupLayouts,
    ) -> wgpu::PipelineLayout {
        if let Some(l) = self.layouts.get(&kind) {
            return l.clone();
        }
        let bgl = match kind {
            PipelineKind::Glyph => vec![Some(&layouts.globals), Some(&layouts.glyph)],
            PipelineKind::Image => vec![Some(&layouts.globals), Some(&layouts.texture)],
            PipelineKind::CompositeOver => vec![Some(&layouts.globals), Some(&layouts.composite)],
            _ => vec![Some(&layouts.globals)],
        };
        let l = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("pebbles.gpu.layout.pipeline"),
            bind_group_layouts: &bgl,
            immediate_size: 0,
        });
        self.layouts.insert(kind, l.clone());
        l
    }

    /// Build (and cache) the pipeline for `key` (K2: ensure, then encode).
    pub(crate) fn ensure(
        &mut self,
        device: &wgpu::Device,
        modules: &ShaderModules,
        layouts: &BindGroupLayouts,
        key: PipelineKey,
        stats: &mut FrameStats,
    ) {
        if self.pipelines.contains_key(&key) {
            return;
        }
        let pipeline_layout = self.layout_for(device, key.kind, layouts);
        let shader = modules.for_kind(key.kind);
        let pipeline = match key.kind {
            PipelineKind::Solid => solid::build(device, shader, &pipeline_layout, key),
            PipelineKind::Shadow => shadow::build(device, shader, &pipeline_layout, key),
            PipelineKind::Sdf => sdf::build(device, shader, &pipeline_layout, key),
            PipelineKind::Glyph => glyph::build(device, shader, &pipeline_layout, key),
            PipelineKind::Image => image::build(device, shader, &pipeline_layout, key),
            PipelineKind::CompositeOver => composite::build(device, shader, &pipeline_layout, key),
            // Later cards add their builders; never reached before then.
            _ => return,
        };
        self.pipelines.insert(key, pipeline);
        stats.pipelines_created += 1;
    }

    pub(crate) fn get(&self, key: PipelineKey) -> Option<&wgpu::RenderPipeline> {
        self.pipelines.get(&key)
    }

    /// Prewarm every Phase 1–3 kind for `(format, sample_count)` on first use (K3).
    pub(crate) fn prewarm(
        &mut self,
        device: &wgpu::Device,
        modules: &ShaderModules,
        layouts: &BindGroupLayouts,
        format: wgpu::TextureFormat,
        sample_count: u32,
        stats: &mut FrameStats,
    ) {
        if !self.prewarmed.insert((format, sample_count)) {
            return;
        }
        let key = |kind, variant| PipelineKey { kind, color_format: format, sample_count, blend: 0, variant };
        for k in [
            key(PipelineKind::Solid, 0),
            key(PipelineKind::Shadow, 0),
            key(PipelineKind::Sdf, 0),
            key(PipelineKind::Glyph, 0),
            key(PipelineKind::Image, 0),
        ] {
            self.ensure(device, modules, layouts, k, stats);
        }
    }
}
