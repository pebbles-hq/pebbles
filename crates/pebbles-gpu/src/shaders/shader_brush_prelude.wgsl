// Shader brushes (09 §6.1, L43): the template around the app's
// `fn pebbles_paint(local: vec2<f32>, device: vec2<f32>) -> vec4<f32>` (premultiplied RGBA).
// Assembled as prelude.wgsl + this file + the app source + brush_cover.wgsl; the app may read
// `u.values`, `u.size`, `t0`, `t1` and `s0` (sample with `textureSampleLevel`).
struct PebblesShaderUniforms {
    // device px → brush local space, rows x and y: (a, c, e, 0), (b, d, f, 0).
    inv0: vec4<f32>,
    inv1: vec4<f32>,
    // The shape's local bounds size (xy).
    size: vec2<f32>,
    _pad: vec2<f32>,
    // The brush's ≤ 64 floats, zero-filled.
    values: array<vec4<f32>, 16>,
};
@group(1) @binding(0) var t0: texture_2d<f32>;
@group(1) @binding(1) var t1: texture_2d<f32>;
@group(1) @binding(2) var s0: sampler;
@group(1) @binding(3) var<uniform> u: PebblesShaderUniforms;

fn brush_paint(p: vec2<f32>) -> vec4<f32> {
    let local = vec2<f32>(dot(u.inv0.xyz, vec3<f32>(p, 1.0)), dot(u.inv1.xyz, vec3<f32>(p, 1.0)));
    return clamp(pebbles_paint(local, p), vec4<f32>(0.0), vec4<f32>(1.0));
}

fn brush_post(c: vec4<f32>, p: vec2<f32>) -> vec4<f32> {
    return c;
}
