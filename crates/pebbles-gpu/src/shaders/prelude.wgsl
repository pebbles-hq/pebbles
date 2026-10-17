// prelude.wgsl: prepended to every pipeline's WGSL by concat!(include_str!(..)).
// Mirrors `Globals` in execute/resources/globals.rs (owner 10 §2.6; 32 bytes). Keep field order and padding identical.
struct Globals {
    target_size: vec2<f32>,
    target_origin: vec2<f32>,
    inv_target_size: vec2<f32>,
    color_flags: u32,
    _pad0: u32,
};
const COLOR_FLAG_DECODE_SRGB_OUTPUT: u32 = 1u;

@group(0) @binding(0) var<uniform> globals: Globals;

// Device px (y down) -> clip space for the current pass target (10 §2.6 owns this function).
fn to_clip(p: vec2<f32>) -> vec4<f32> {
    let q = (p - globals.target_origin) * globals.inv_target_size;
    return vec4<f32>(q.x * 2.0 - 1.0, 1.0 - q.y * 2.0, 0.0, 1.0);
}

// Unit-quad corner for instanced 4-vertex triangle strips: 0 (0,0), 1 (1,0), 2 (0,1), 3 (1,1).
fn quad_corner(vertex_index: u32) -> vec2<f32> {
    return vec2<f32>(f32(vertex_index & 1u), f32((vertex_index >> 1u) & 1u));
}
