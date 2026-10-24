// Image brushes (09 §6, L42): fills and strokes painted with an image under its paint
// transform, filtered in code by the brush sampler key (`sample_portable`, DR-21; explicit
// LOD: uniform-control-flow safe, `textureLod` on WebGL2). Coverage variants: brush_cover.wgsl.
struct ImageBrushParams {
    m0: vec4<f32>,   // D row x: device px → image px; w = SamplerKey::mode
    m1: vec4<f32>,   // D row y
    size: vec4<f32>, // (w, h, 1/w, 1/h) of the logical image
};
@group(1) @binding(0) var brush_tex: texture_2d<f32>;
@group(1) @binding(1) var brush_sampler: sampler;
@group(1) @binding(2) var<uniform> bparams: ImageBrushParams;

fn brush_paint(p: vec2<f32>) -> vec4<f32> {
    let px = vec2<f32>(dot(bparams.m0.xyz, vec3<f32>(p, 1.0)), dot(bparams.m1.xyz, vec3<f32>(p, 1.0)));
    return sample_portable(brush_tex, px * bparams.size.zw, 0.0, 0.0, u32(bparams.m0.w));
}

fn brush_post(c: vec4<f32>, p: vec2<f32>) -> vec4<f32> {
    return c;
}
