// Blit (09 §4.4): copy the root color texture into the frame target (which may be a swapchain
// image that cannot be a copy destination). Fullscreen triangle, `textureLoad`, `blend: None`.
@group(1) @binding(0) var blit_src: texture_2d<f32>;

@vertex
fn vs_blit(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    return vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
}

@fragment
fn fs_blit(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(blit_src, vec2<i32>(floor(frag_coord.xy)), 0);
}
