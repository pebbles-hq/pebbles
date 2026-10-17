// Instanced image quads (06 §9.4). Texels are premultiplied at upload (02 §3); the shader only
// multiplies by the instance opacity.
@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

struct QuadIn {
    @location(0) xform: vec4<f32>,
    @location(1) origin: vec2<f32>,
    @location(2) uv: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(4) layer_kind: vec2<u32>,
}

struct QuadOut {
    @builtin(position) p: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) opacity: f32,
}

@vertex
fn vs(i: QuadIn, @builtin(vertex_index) vi: u32) -> QuadOut {
    let c = quad_corner(vi);
    var o: QuadOut;
    o.p = to_clip(i.origin + c.x * i.xform.xy + c.y * i.xform.zw);
    o.uv = mix(i.uv.xy, i.uv.zw, c);
    o.opacity = i.color.a;
    return o;
}

@fragment
fn fs(i: QuadOut) -> @location(0) vec4<f32> {
    return textureSample(image, image_sampler, i.uv) * i.opacity;
}
