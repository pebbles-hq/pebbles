//! The pipeline descriptor table (02 §9, R50): the single source of truth for every
//! pipeline's shader file, entry points, topology and vertex layout. `build` reads it.

use std::mem::size_of;

use super::{PipelineKey, PipelineKind};
use crate::gpu_types::{QuadInstance, ShadowInstance, ShapeInstance, SolidVertex};

const SOLID_ATTRIBUTES: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4];
/// Glyph and image quads (`QuadInstance`).
pub(crate) const QUAD_ATTRIBUTES: [wgpu::VertexAttribute; 5] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x2, 2 => Float32x4, 3 => Float32x4, 4 => Uint32x2];
const SHADOW_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4];

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
        PipelineDesc {
            kind: PipelineKind::CompositeOver,
            wgsl: "composite.wgsl",
            vs: "vs_main",
            fs: "fs_over",
            topology: TriangleStrip,
            layout: None,
            group1: true,
        },
    ]
}

/// The table row of `kind`, if the kind exists in code.
pub(crate) fn desc_of(kind: PipelineKind) -> Option<PipelineDesc> {
    pipeline_descs().into_iter().find(|d| d.kind == kind)
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
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
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

#[cfg(test)]
#[path = "descs_tests.rs"]
mod tests;
