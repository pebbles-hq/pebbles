//! The CPU↔GPU vertex/instance data structs, uploaded verbatim to the GPU. (`Vertex`
//! is renamed `SolidVertex`, `position` → `pos`, per 06 §12.2 B6.)

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

/// A shadow quad vertex: the inflated quad corner plus the (uninflated) rounded
/// rect + blur parameters, so all shadows share one draw call.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct ShadowVertex {
    pub position: [f32; 2],
    pub rect: [f32; 4],
    pub radius: f32,
    pub sigma: f32,
    pub color: [f32; 4],
}
