// Composite a single-sample layer texture into the current target (09 §3.7). No sampler:
// textureLoad with integer texel coordinates (1:1 pixel mapping, WebGL2 texelFetch).
struct CompositeParams {
    bounds: vec4<f32>,
    alpha: f32,
    mix_mode: u32,
    compose: u32,
    _pad: u32,
};
@group(1) @binding(0) var layer_tex: texture_2d<f32>;
@group(1) @binding(1) var<uniform> comp: CompositeParams;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32(vi & 1u), f32(vi >> 1u));
    return to_clip(mix(comp.bounds.xy, comp.bounds.zw, corner));
}

fn layer_texel(p: vec2<f32>) -> vec4<f32> {
    return textureLoad(layer_tex, vec2<i32>(floor(p - comp.bounds.xy)), 0);
}

// CompositeOver: premultiplied source-over (BlendState::PREMULTIPLIED_ALPHA_BLENDING). The only
// fragment without `finish_color`: the layer's texels already passed through it when they were
// drawn, so a second decode would double-convert on the GL sRGB path (R13).
// `clip_coverage` arrives with L04 and `clip_mask_coverage` with L08.
@fragment
fn fs_over(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    return layer_texel(device_pos(frag_coord)) * comp.alpha;
}
