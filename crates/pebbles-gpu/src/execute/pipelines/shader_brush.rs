//! Shader-brush pipelines (09 §6.1, L43): the image brush's coverage rows (Shape, Mask, Mesh;
//! 02 §9) with the program's module and the shader-brush group 1.

use super::{PipelineKey, PipelineKind, descs};
use crate::execute::resources::bind_groups::BindGroupLayouts;

pub(crate) fn pipeline_layout(device: &wgpu::Device, layouts: &BindGroupLayouts) -> wgpu::PipelineLayout {
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("pebbles.gpu.layout.pipeline.shader_brush"),
        bind_group_layouts: &[Some(&layouts.globals), Some(&layouts.shader_brush)],
        immediate_size: 0,
    })
}

/// The `variant` (0 Shape, 1 Mask, 2 Mesh) pipeline of `module`.
pub(crate) fn build(
    device: &wgpu::Device,
    module: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    variant: u8,
    format: wgpu::TextureFormat,
    samples: u32,
) -> Option<wgpu::RenderPipeline> {
    let key = PipelineKey {
        kind: PipelineKind::ImageBrush,
        color_format: format,
        sample_count: samples,
        blend: 0,
        variant,
    };
    let desc = descs::desc_for(key)?;
    Some(descs::build(device, module, layout, key, &desc))
}
