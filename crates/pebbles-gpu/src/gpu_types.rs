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

/// A textured quad vertex for the glyph pipeline: physical-pixel position + atlas UV +
/// a premultiplied tint color.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct GlyphVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

/// A textured quad vertex: physical-pixel position + source-image UV.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct ImageVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
}

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
