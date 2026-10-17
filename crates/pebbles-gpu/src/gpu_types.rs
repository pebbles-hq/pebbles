//! The CPU↔GPU vertex/instance data structs, uploaded verbatim to the GPU. (`Vertex`
//! is renamed `SolidVertex`, `position` → `pos`, per 06 §12.2 B6.)

use std::mem::size_of;

use bytemuck::{Pod, Zeroable};

/// Solid fill/stroke vertex (Phase-C triangles).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct SolidVertex {
    pub pos: [f32; 2],
    pub color: [f32; 4],
}

/// Textured quad (glyph; images from C13); instanced strip. Corner c ∈ {(0,0),(1,0),(0,1),(1,1)}
/// maps to `origin + c.x * (xform.x, xform.y) + c.y * (xform.z, xform.w)`.
/// WGSL: `@location(0) xform: vec4<f32>, @location(1) origin: vec2<f32>, @location(2) uv: vec4<f32>,
///        @location(3) color: vec4<f32>, @location(4) layer_kind: vec2<u32>`
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct QuadInstance {
    /// Linear map of the unit quad to device px; axis-aligned w×h quad = [w, 0, 0, h].
    pub xform: [f32; 4],
    /// Device-px position of corner (0, 0).
    pub origin: [f32; 2],
    /// Atlas texel rect (x0, y0, x1, y1); the shader normalizes by the texture size (10 §5.9).
    pub uv: [f32; 4],
    /// Mask glyph: premultiplied tint. Color glyph: only `.a` (opacity) is read.
    pub color: [f32; 4],
    /// [array layer, kind]; kind 0 = mask glyph, 1 = color glyph, 2 = image.
    pub layer_kind: [u32; 2],
}
const _: () = assert!(size_of::<QuadInstance>() == 64);
const _: () = assert!(core::mem::offset_of!(QuadInstance, origin) == 16);
const _: () = assert!(core::mem::offset_of!(QuadInstance, uv) == 24);
const _: () = assert!(core::mem::offset_of!(QuadInstance, color) == 40);
const _: () = assert!(core::mem::offset_of!(QuadInstance, layer_kind) == 56);

/// Blurred rounded-rect shadow; instanced strip, quad inflated by 3σ in the vertex shader.
/// WGSL: `@location(0) rect: vec4<f32>, @location(1) color: vec4<f32>, @location(2) params: vec4<f32>`
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct ShadowInstance {
    /// Device-space bounds (x0, y0, x1, y1) of the un-blurred shape.
    pub rect: [f32; 4],
    /// Premultiplied color.
    pub color: [f32; 4],
    /// (corner radius px, sigma px, 0, 0).
    pub params: [f32; 4],
}

const _: () = assert!(size_of::<ShadowInstance>() == 48);
const _: () = assert!(std::mem::offset_of!(ShadowInstance, color) == 16);
const _: () = assert!(std::mem::offset_of!(ShadowInstance, params) == 32);

/// Tier A analytic shape (02 §6.4): rect / rounded rect / circle, fill or uniform stroke, as one
/// instanced quad with SDF coverage. WGSL: five `vec4<f32>` attributes at locations 0–4.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct ShapeInstance {
    /// Device px: x0, y0, x1, y1.
    pub outer: [f32; 4],
    /// Device px: top-left, top-right, bottom-right, bottom-left.
    pub outer_radii: [f32; 4],
    /// Stroke: inner rect; fill: `[0.0; 4]`.
    pub inner: [f32; 4],
    pub inner_radii: [f32; 4],
    /// Premultiplied sRGB-encoded (02 §3), hairline alpha folded in.
    pub color: [f32; 4],
}
const _: () = assert!(size_of::<ShapeInstance>() == 80);

impl ShapeInstance {
    pub(crate) const ATTRIBUTES: [wgpu::VertexAttribute; 5] = wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4];

    /// The instance-step vertex buffer layout (stride 80).
    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &Self::ATTRIBUTES,
        }
    }
}
