//! Pipeline families, the pipeline cache and the once-per-Renderer shader modules.

use std::collections::{HashMap, HashSet};

use super::resources::bind_groups::BindGroupLayouts;
use crate::stats::FrameStats;

pub(crate) mod descs;

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
    ImageBrush,
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
    /// Every kind, in declaration order.
    pub(crate) const ALL: [PipelineKind; 19] = [
        Self::Solid,
        Self::Sdf,
        Self::Shadow,
        Self::Glyph,
        Self::Image,
        Self::Gradient,
        Self::ImageBrush,
        Self::CompositeOver,
        Self::CompositeCompose,
        Self::CompositeBackdrop,
        Self::Blit,
        Self::Vertices,
        Self::Sprite,
        Self::Blur,
        Self::ColorMatrix,
        Self::Morphology,
        Self::Projected,
        Self::ShaderBrush,
        Self::MaskMultiply,
    ];

    /// K3: created on the first render to a `(format, sample_count)` (the Phase 1–3 kinds);
    /// Phase-4 kinds are created lazily.
    pub(crate) const fn prewarm(self) -> bool {
        matches!(self, Self::Solid | Self::Glyph | Self::Image | Self::Shadow | Self::Sdf)
    }

    /// `pebbles.gpu.pipeline.<kind>` descriptor label (S51); static.
    pub(crate) const fn pipeline_label(self) -> &'static str {
        match self {
            Self::Solid => "pebbles.gpu.pipeline.solid",
            Self::Sdf => "pebbles.gpu.pipeline.sdf",
            Self::Shadow => "pebbles.gpu.pipeline.shadow",
            Self::Glyph => "pebbles.gpu.pipeline.glyph",
            Self::Image => "pebbles.gpu.pipeline.image",
            Self::CompositeOver => "pebbles.gpu.pipeline.composite_over",
            Self::CompositeCompose => "pebbles.gpu.pipeline.composite_compose",
            Self::CompositeBackdrop => "pebbles.gpu.pipeline.composite_backdrop",
            Self::Blit => "pebbles.gpu.pipeline.blit",
            Self::Gradient => "pebbles.gpu.pipeline.gradient",
            Self::ImageBrush => "pebbles.gpu.pipeline.image_brush",
            Self::Blur => "pebbles.gpu.pipeline.blur",
            Self::ColorMatrix => "pebbles.gpu.pipeline.color_matrix",
            Self::Morphology => "pebbles.gpu.pipeline.morphology",
            Self::Projected => "pebbles.gpu.pipeline.projected",
            _ => "pebbles.gpu.pipeline.other",
        }
    }

    /// `pebbles.gpu.draw.<kind>` debug-group label (11 §4.4); static, never allocated per frame.
    pub(crate) const fn debug_label(self) -> &'static str {
        match self {
            Self::Solid => "pebbles.gpu.draw.solid",
            Self::Sdf => "pebbles.gpu.draw.sdf",
            Self::Shadow => "pebbles.gpu.draw.shadow",
            Self::Glyph => "pebbles.gpu.draw.glyph",
            Self::Image => "pebbles.gpu.draw.image",
            Self::Gradient => "pebbles.gpu.draw.gradient",
            Self::ImageBrush => "pebbles.gpu.draw.image_brush",
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
    pub blit: wgpu::ShaderModule,
    pub gradient: wgpu::ShaderModule,
    pub image_brush: wgpu::ShaderModule,
    pub effects: wgpu::ShaderModule,
    pub projected: wgpu::ShaderModule,
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
            blit: module("pebbles.gpu.shader.blit", shaders::BLIT),
            gradient: module("pebbles.gpu.shader.gradient", shaders::GRADIENT),
            image_brush: module("pebbles.gpu.shader.image_brush", shaders::IMAGE_BRUSH),
            effects: module("pebbles.gpu.shader.effects", shaders::EFFECTS),
            projected: module("pebbles.gpu.shader.projected", shaders::PROJECTED),
        }
    }

    pub(crate) fn for_kind(&self, kind: PipelineKind) -> &wgpu::ShaderModule {
        match kind {
            PipelineKind::Solid => &self.solid,
            PipelineKind::Glyph => &self.glyph,
            PipelineKind::Image => &self.image,
            PipelineKind::Shadow => &self.shadow,
            PipelineKind::Sdf => &self.sdf,
            PipelineKind::CompositeOver
            | PipelineKind::CompositeCompose
            | PipelineKind::CompositeBackdrop => &self.composite,
            PipelineKind::Blit => &self.blit,
            PipelineKind::Gradient => &self.gradient,
            PipelineKind::ImageBrush => &self.image_brush,
            PipelineKind::Blur | PipelineKind::ColorMatrix | PipelineKind::Morphology => &self.effects,
            PipelineKind::Projected => &self.projected,
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
    /// Created pipeline count (census).
    #[cfg(debug_assertions)]
    pub(crate) fn len(&self) -> usize {
        self.pipelines.len()
    }

    pub(crate) fn new() -> Self {
        Self { layouts: HashMap::new(), pipelines: HashMap::new(), prewarmed: HashSet::new() }
    }

    fn layout_for(
        &mut self,
        device: &wgpu::Device,
        desc: &descs::PipelineDesc,
        layouts: &BindGroupLayouts,
    ) -> wgpu::PipelineLayout {
        let kind = desc.kind;
        if let Some(l) = self.layouts.get(&kind) {
            return l.clone();
        }
        let group1 = match kind {
            PipelineKind::Glyph => Some(&layouts.glyph),
            PipelineKind::Image => Some(&layouts.texture),
            PipelineKind::CompositeOver | PipelineKind::CompositeCompose => Some(&layouts.composite),
            PipelineKind::CompositeBackdrop => Some(&layouts.composite_backdrop),
            PipelineKind::Blit => Some(&layouts.blit),
            PipelineKind::Gradient => Some(&layouts.gradient),
            PipelineKind::ImageBrush => Some(&layouts.image_brush),
            PipelineKind::Blur | PipelineKind::ColorMatrix | PipelineKind::Morphology => {
                Some(&layouts.effect)
            }
            PipelineKind::Projected => Some(&layouts.projected),
            _ => None,
        };
        debug_assert_eq!(group1.is_some(), desc.group1, "{kind:?}: group 1 disagrees with 02 §9");
        let bgl: Vec<_> =
            std::iter::once(Some(&layouts.globals)).chain(group1.filter(|_| desc.group1).map(Some)).collect();
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
        let shader = modules.for_kind(key.kind);
        // Kinds without a §9 row in code yet are never reached (later cards add them).
        let Some(desc) = descs::desc_for(key) else { return };
        debug_assert!(crate::shaders::ALL.iter().any(|(n, _)| desc.wgsl.trim_end_matches(".wgsl") == *n));
        let pipeline_layout = self.layout_for(device, &desc, layouts);
        let pipeline = descs::build(device, shader, &pipeline_layout, key, &desc);
        self.pipelines.insert(key, pipeline);
        stats.pipelines_created += 1;
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
        for kind in PipelineKind::ALL.into_iter().filter(|k| k.prewarm()) {
            let key = PipelineKey { kind, color_format: format, sample_count, blend: 0, variant: 0 };
            self.ensure(device, modules, layouts, key, stats);
        }
    }

    /// K2: the pipeline for `key`, which `ensure` must have created before the pass opened. A
    /// miss is an internal invariant violation: asserted in debug, logged, and the draw skipped.
    pub(crate) fn get_ensured(&self, key: PipelineKey) -> Option<&wgpu::RenderPipeline> {
        let p = self.pipelines.get(&key);
        if p.is_none() {
            debug_assert!(false, "pipeline {key:?} was not ensured before encoding (K2)");
            pebbles_foundation::log::logf(
                pebbles_foundation::log::Level::Error,
                pebbles_foundation::log::Cat::Gpu,
                format_args!("pipeline {key:?} missing at encode; draw skipped (K2)"),
            );
        }
        p
    }
}
