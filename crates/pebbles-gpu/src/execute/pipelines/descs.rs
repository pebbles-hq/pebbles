//! The pipeline descriptor table (02 §9, R50): the single source of truth for every
//! pipeline's shader file, entry points, topology and vertex layout. `build` reads it.

use std::mem::size_of;

use super::{PipelineKey, PipelineKind};
use crate::gpu_types::{QuadInstance, ShadowInstance, ShapeInstance, SolidVertex};

const SOLID_ATTRIBUTES: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];
/// Glyph and image quads (`QuadInstance`).
pub(crate) const QUAD_ATTRIBUTES: [wgpu::VertexAttribute; 5] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x2, 2 => Float32x4, 3 => Float32x4, 4 => Uint32x2];
const SHADOW_ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    0 => Float32x4,
    1 => Float32x4,
    2 => Float32x4,
    3 => Float32x4,
    4 => Float32x4,
    5 => Float32x4
];

/// One row of 02 §9.
#[derive(Clone, Debug)]
pub(crate) struct PipelineDesc {
    pub kind: PipelineKind,
    /// WGSL file name under `src/shaders/` (prelude prepended).
    pub wgsl: &'static str,
    pub vs: &'static str,
    pub fs: &'static str,
    pub topology: wgpu::PrimitiveTopology,
    /// `None`: no vertex buffers (corners from a uniform).
    pub layout: Option<wgpu::VertexBufferLayout<'static>>,
    /// Whether the pipeline binds a group 1.
    pub group1: bool,
}

const fn layout(
    stride: usize,
    step_mode: wgpu::VertexStepMode,
    attributes: &'static [wgpu::VertexAttribute],
) -> Option<wgpu::VertexBufferLayout<'static>> {
    Some(wgpu::VertexBufferLayout { array_stride: stride as u64, step_mode, attributes })
}

/// Every 02 §9 row that exists in code.
pub(crate) fn pipeline_descs() -> Vec<PipelineDesc> {
    use wgpu::PrimitiveTopology::{TriangleList, TriangleStrip};
    use wgpu::VertexStepMode::{Instance, Vertex};
    let quad = layout(size_of::<QuadInstance>(), Instance, &QUAD_ATTRIBUTES);
    vec![
        PipelineDesc {
            kind: PipelineKind::Solid,
            wgsl: "solid.wgsl",
            vs: "vs",
            fs: "fs",
            topology: TriangleList,
            layout: layout(size_of::<SolidVertex>(), Vertex, &SOLID_ATTRIBUTES),
            group1: false,
        },
        PipelineDesc {
            kind: PipelineKind::Sdf,
            wgsl: "sdf.wgsl",
            vs: "vs_shape",
            fs: "fs_shape",
            topology: TriangleStrip,
            layout: layout(size_of::<ShapeInstance>(), Instance, &ShapeInstance::ATTRIBUTES),
            group1: false,
        },
        PipelineDesc {
            kind: PipelineKind::Glyph,
            wgsl: "glyph.wgsl",
            vs: "vs",
            fs: "fs_main",
            topology: TriangleStrip,
            layout: quad.clone(),
            group1: true,
        },
        PipelineDesc {
            kind: PipelineKind::Image,
            wgsl: "image.wgsl",
            vs: "vs",
            fs: "fs",
            topology: TriangleStrip,
            layout: quad,
            group1: true,
        },
        PipelineDesc {
            kind: PipelineKind::Shadow,
            wgsl: "shadow.wgsl",
            vs: "vs",
            fs: "fs",
            topology: TriangleStrip,
            layout: layout(size_of::<ShadowInstance>(), Instance, &SHADOW_ATTRIBUTES),
            group1: false,
        },
        // 09 §6 brush × tier rule: each brush kind's three coverage variants (key.variant
        // 0 Shape, 1 Mask, 2 Mesh); `desc_for` picks the row by variant.
        PipelineDesc {
            kind: PipelineKind::Gradient,
            wgsl: "gradient.wgsl",
            vs: "vs_shape",
            fs: "fs_shape",
            topology: TriangleStrip,
            layout: layout(size_of::<ShapeInstance>(), Instance, &ShapeInstance::ATTRIBUTES),
            group1: true,
        },
        PipelineDesc {
            kind: PipelineKind::Gradient,
            wgsl: "gradient.wgsl",
            vs: "vs_mask",
            fs: "fs_mask",
            topology: TriangleStrip,
            layout: layout(size_of::<QuadInstance>(), Instance, &QUAD_ATTRIBUTES),
            group1: true,
        },
        PipelineDesc {
            kind: PipelineKind::Gradient,
            wgsl: "gradient.wgsl",
            vs: "vs_mesh",
            fs: "fs_mesh",
            topology: TriangleList,
            layout: layout(size_of::<SolidVertex>(), Vertex, &SOLID_ATTRIBUTES),
            group1: true,
        },
        PipelineDesc {
            kind: PipelineKind::ImageBrush,
            wgsl: "image_brush.wgsl",
            vs: "vs_shape",
            fs: "fs_shape",
            topology: TriangleStrip,
            layout: layout(size_of::<ShapeInstance>(), Instance, &ShapeInstance::ATTRIBUTES),
            group1: true,
        },
        PipelineDesc {
            kind: PipelineKind::ImageBrush,
            wgsl: "image_brush.wgsl",
            vs: "vs_mask",
            fs: "fs_mask",
            topology: TriangleStrip,
            layout: layout(size_of::<QuadInstance>(), Instance, &QUAD_ATTRIBUTES),
            group1: true,
        },
        PipelineDesc {
            kind: PipelineKind::ImageBrush,
            wgsl: "image_brush.wgsl",
            vs: "vs_mesh",
            fs: "fs_mesh",
            topology: TriangleList,
            layout: layout(size_of::<SolidVertex>(), Vertex, &SOLID_ATTRIBUTES),
            group1: true,
        },
        PipelineDesc {
            kind: PipelineKind::Blur,
            wgsl: "effects.wgsl",
            vs: "vs_full",
            fs: "fs_down",
            topology: TriangleList,
            layout: None,
            group1: true,
        },
        PipelineDesc {
            kind: PipelineKind::Blur,
            wgsl: "effects.wgsl",
            vs: "vs_full",
            fs: "fs_blur",
            topology: TriangleList,
            layout: None,
            group1: true,
        },
        PipelineDesc {
            kind: PipelineKind::Blur,
            wgsl: "effects.wgsl",
            vs: "vs_full",
            fs: "fs_up",
            topology: TriangleList,
            layout: None,
            group1: true,
        },
        PipelineDesc {
            kind: PipelineKind::ColorMatrix,
            wgsl: "effects.wgsl",
            vs: "vs_full",
            fs: "fs_matrix",
            topology: TriangleList,
            layout: None,
            group1: true,
        },
        PipelineDesc {
            kind: PipelineKind::Morphology,
            wgsl: "effects.wgsl",
            vs: "vs_full",
            fs: "fs_dilate",
            topology: TriangleList,
            layout: None,
            group1: true,
        },
        PipelineDesc {
            kind: PipelineKind::Morphology,
            wgsl: "effects.wgsl",
            vs: "vs_full",
            fs: "fs_erode",
            topology: TriangleList,
            layout: None,
            group1: true,
        },
        PipelineDesc {
            kind: PipelineKind::CompositeOver,
            wgsl: "composite.wgsl",
            vs: "vs_main",
            fs: "fs_over",
            topology: TriangleStrip,
            layout: None,
            group1: true,
        },
        PipelineDesc {
            kind: PipelineKind::CompositeCompose,
            wgsl: "composite.wgsl",
            vs: "vs_main",
            fs: "fs_compose",
            topology: TriangleStrip,
            layout: None,
            group1: true,
        },
        PipelineDesc {
            kind: PipelineKind::CompositeBackdrop,
            wgsl: "composite.wgsl",
            vs: "vs_main",
            fs: "fs_backdrop",
            topology: TriangleStrip,
            layout: None,
            group1: true,
        },
        PipelineDesc {
            kind: PipelineKind::Blit,
            wgsl: "blit.wgsl",
            vs: "vs_blit",
            fs: "fs_blit",
            topology: TriangleList,
            layout: None,
            group1: true,
        },
    ]
}

/// The table row of `kind`, if the kind exists in code (a variant kind: its first row).
pub(crate) fn desc_of(kind: PipelineKind) -> Option<PipelineDesc> {
    pipeline_descs().into_iter().find(|d| d.kind == kind)
}

/// The row `key` builds: kinds with coverage variants (Gradient) select by `key.variant`.
pub(crate) fn desc_for(key: PipelineKey) -> Option<PipelineDesc> {
    match key.kind {
        PipelineKind::Gradient | PipelineKind::ImageBrush | PipelineKind::Blur | PipelineKind::Morphology => {
            pipeline_descs().into_iter().filter(|d| d.kind == key.kind).nth(usize::from(key.variant))
        }
        kind => desc_of(kind),
    }
}

/// Build `key`'s pipeline from its table row (premultiplied source-over, no depth/stencil,
/// `cull_mode: None`, multisample = `key.sample_count`; 02 §9).
pub(crate) fn build(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    key: PipelineKey,
    desc: &PipelineDesc,
) -> wgpu::RenderPipeline {
    let buffers = desc.layout.as_slice();
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(desc.kind.pipeline_label()),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(desc.vs),
            compilation_options: Default::default(),
            buffers,
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(desc.fs),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: key.color_format,
                blend: match key.kind {
                    PipelineKind::CompositeCompose => Some(compose_blend_state(compose_of(key.blend))),
                    // The shader computes the final pixel (09 §4.4–§4.5).
                    // The shader computes the final pixel (09 §4.4–§4.5); effect passes replace.
                    PipelineKind::CompositeBackdrop
                    | PipelineKind::Blit
                    | PipelineKind::Blur
                    | PipelineKind::ColorMatrix
                    | PipelineKind::Morphology => None,
                    _ => Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                },
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState { topology: desc.topology, ..Default::default() },
        depth_stencil: None,
        multisample: wgpu::MultisampleState { count: key.sample_count, ..Default::default() },
        multiview_mask: None,
        cache: None,
    })
}

/// The fixed-function Porter-Duff blend state of `compose` (09 §4.3): with premultiplied
/// colors `out = src·Fa + dst·Fb` is the W3C form; the Unorm target clamps `Plus*` to 1.
pub(crate) fn compose_blend_state(compose: peniko::Compose) -> wgpu::BlendState {
    use peniko::Compose as P;
    use wgpu::BlendFactor as F;
    let (src_factor, dst_factor) = match compose {
        P::Clear => (F::Zero, F::Zero),
        P::Copy => (F::One, F::Zero),
        P::Dest => (F::Zero, F::One),
        P::SrcOver => (F::One, F::OneMinusSrcAlpha),
        P::DestOver => (F::OneMinusDstAlpha, F::One),
        P::SrcIn => (F::DstAlpha, F::Zero),
        P::DestIn => (F::Zero, F::SrcAlpha),
        P::SrcOut => (F::OneMinusDstAlpha, F::Zero),
        P::DestOut => (F::Zero, F::OneMinusSrcAlpha),
        P::SrcAtop => (F::DstAlpha, F::OneMinusSrcAlpha),
        P::DestAtop => (F::OneMinusDstAlpha, F::SrcAlpha),
        P::Xor => (F::OneMinusDstAlpha, F::OneMinusSrcAlpha),
        P::Plus | P::PlusLighter => (F::One, F::One),
    };
    let c = wgpu::BlendComponent { src_factor, dst_factor, operation: wgpu::BlendOperation::Add };
    wgpu::BlendState { color: c, alpha: c }
}

/// `peniko::Compose` from its `repr(u8)` (`PipelineKey.blend`); unknown values are SrcOver.
pub(crate) fn compose_of(v: u8) -> peniko::Compose {
    use peniko::Compose as P;
    const ALL: [P; 14] = [
        P::Clear,
        P::Copy,
        P::Dest,
        P::SrcOver,
        P::DestOver,
        P::SrcIn,
        P::DestIn,
        P::SrcOut,
        P::DestOut,
        P::SrcAtop,
        P::DestAtop,
        P::Xor,
        P::Plus,
        P::PlusLighter,
    ];
    ALL.get(usize::from(v)).copied().unwrap_or(P::SrcOver)
}

#[cfg(test)]
#[path = "descs_tests.rs"]
mod tests;
