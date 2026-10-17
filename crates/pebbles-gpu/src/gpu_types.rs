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

/// A textured quad vertex: physical-pixel position + source-image UV.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct ImageVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    /// Group alpha × sampler alpha, multiplied into the premultiplied sample (C17).
    pub opacity: f32,
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
