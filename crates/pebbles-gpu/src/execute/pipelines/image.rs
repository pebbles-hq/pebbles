//! The image pipeline: a textured quad sampling an uploaded `Rgba8` image.

use std::mem::size_of;

use super::{PipelineKey, PipelineKind};
use crate::gpu_types::QuadInstance;

pub(crate) fn build(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    key: PipelineKey,
) -> wgpu::RenderPipeline {
    debug_assert_eq!(key.kind, PipelineKind::Image);
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("pebbles.gpu.pipeline.image"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: size_of::<QuadInstance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &super::glyph::QUAD_ATTRIBUTES,
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs"),
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
