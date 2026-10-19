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
    /// a, b, c, d: the linear part of M (local → device).
    pub xform: [f32; 4],
    /// xy = M · (rect.x0, rect.y0) in device px; zw = (width, height) local.
    pub origin_size: [f32; 4],
    /// exponent, recip_exponent, scale, std_dev_inv (02 §8.2).
    pub params0: [f32; 4],
    /// min_edge, w, h, r1.
    pub params1: [f32; 4],
    /// Premultiplied (02 §3).
    pub color: [f32; 4],
    /// x = 2.5 · std_dev (local units, quad inflation); yzw = 0.
    pub kernel: [f32; 4],
}

const _: () = assert!(size_of::<ShadowInstance>() == 96);
const _: () = assert!(std::mem::offset_of!(ShadowInstance, color) == 64);

/// One SDF clip record (09 §2.4): bound at group 0, binding 1, dynamic offset.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct ClipParams {
    /// Device px: x0, y0, x1, y1.
    pub rect: [f32; 4],
    /// Device px: tl, tr, br, bl.
    pub radii: [f32; 4],
}

const _: () = assert!(size_of::<ClipParams>() == 32);

#[cfg(test)]
#[test]
fn clip_params_layout() {
    assert_eq!(size_of::<ClipParams>(), 32);
    assert_eq!(std::mem::offset_of!(ClipParams, radii), 16);
}

/// One mask-clip record (09 L08): group 0, binding 2, dynamic offset. `atlas.w == 0` = none.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct MaskClipParams {
    /// Device px: x0, y0, width, height of the clip rect.
    pub rect: [f32; 4],
    /// Atlas texel x, y, layer, enabled (1) / none (0).
    pub atlas: [f32; 4],
}

const _: () = assert!(size_of::<MaskClipParams>() == 32);

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
}

/// Per-composite uniform (09 §3.7): one 256-byte-aligned slot per composited Tier C run / layer.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct CompositeParams {
    /// Device px x0, y0, x1, y1 (= layer bounds).
    pub bounds: [f32; 4],
    /// Group opacity (0, 1].
    pub alpha: f32,
    /// `peniko::Mix as u8 as u32`.
    pub mix_mode: u32,
    /// `peniko::Compose as u8 as u32`.
    pub compose: u32,
    pub _pad: u32,
}
const _: () = assert!(size_of::<CompositeParams>() == 32);

/// Gradient draw parameters (09 §5.4), one uniform slot per `GradientDraw`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct GradientParams {
    /// `G` row x: (a, c, e, 0) — device px → gradient space.
    pub m0: [f32; 4],
    /// `G` row y: (b, d, f, 0).
    pub m1: [f32; 4],
    /// (kind, extend as u32, lut_start, lut_len).
    pub info: [u32; 4],
    pub p0: [f32; 4],
    pub p1: [f32; 4],
}
const _: () = assert!(size_of::<GradientParams>() == 80);
const _: () = assert!(core::mem::offset_of!(GradientParams, info) == 32);

/// Image-brush parameters (09 §6), one uniform slot per `ImageBrushDraw`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct ImageBrushParams {
    /// `D` row x: (a, c, e, 0) — device px → image px.
    pub m0: [f32; 4],
    /// `D` row y: (b, d, f, 0).
    pub m1: [f32; 4],
    /// (w, h, 1/w, 1/h) of the logical image.
    pub size: [f32; 4],
}
const _: () = assert!(size_of::<ImageBrushParams>() == 48);

#[cfg(test)]
mod tests {
    use super::{CompositeParams, GradientParams};

    #[test]
    fn composite_params_layout() {
        assert_eq!(size_of::<CompositeParams>(), 32);
        assert_eq!(core::mem::offset_of!(CompositeParams, alpha), 16);
        assert_eq!(core::mem::offset_of!(CompositeParams, mix_mode), 20);
        assert_eq!(core::mem::offset_of!(CompositeParams, compose), 24);
        assert_eq!(core::mem::offset_of!(CompositeParams, _pad), 28);
    }

    #[test]
    fn gradient_params_layout() {
        assert_eq!(size_of::<GradientParams>(), 80);
        assert_eq!(core::mem::offset_of!(GradientParams, info), 32);
        assert_eq!(core::mem::offset_of!(GradientParams, p0), 48);
    }
}
