//! The Tier A shape pipeline (R10): instanced SDF rect / rounded rect / circle quads.

use super::{PipelineKey, PipelineKind};
use crate::gpu_types::ShapeInstance;

pub(crate) fn build(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    key: PipelineKey,
) -> wgpu::RenderPipeline {
    debug_assert_eq!(key.kind, PipelineKind::Sdf);
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("pebbles.gpu.pipeline.sdf"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_shape"),
            compilation_options: Default::default(),
            buffers: &[ShapeInstance::layout()],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_shape"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: key.color_format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState { count: key.sample_count, ..Default::default() },
        multiview_mask: None,
        cache: None,
    })
}
