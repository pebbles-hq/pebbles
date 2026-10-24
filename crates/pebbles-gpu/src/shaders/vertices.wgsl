// Triangle meshes (02 §12, R63): device-space triangle lists, NOT antialiased (pixel centers
// inside a triangle, the rasterizer's top-left rule) — Flutter `drawVertices` semantics. The
// vertex color is premultiplied and already multiplied by the solid brush and group alpha;
// the texture is the image brush (Modulate) or a 1×1 white texel.
@group(1) @binding(0) var mesh_tex: texture_2d<f32>;
@group(1) @binding(1) var mesh_sampler: sampler;
@group(1) @binding(2) var<uniform> mesh_mode: vec4<u32>;

struct MeshIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
}

struct MeshOut {
    @builtin(position) p: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}

@vertex
fn vs(i: MeshIn) -> MeshOut {
    var o: MeshOut;
    o.p = to_clip(i.pos);
    o.uv = i.uv;
    o.color = i.color;
    return o;
}

@fragment
fn fs(i: MeshOut) -> @location(0) vec4<f32> {
    let t = sample_portable(mesh_tex, i.uv, 0.0, 0.0, mesh_mode.x);
    let p = device_pos(i.p);
    return finish_color(t * i.color * (clip_coverage(p) * clip_mask_coverage(p)));
}
