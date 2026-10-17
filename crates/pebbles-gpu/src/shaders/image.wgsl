@group(1) @binding(0) var image: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

struct I {
    @location(0) p: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) opacity: f32,
}

struct O {
    @builtin(position) p: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) opacity: f32,
}

@vertex
fn vs(i: I) -> O {
    var o: O;
    o.p = to_clip(i.p);
    o.uv = i.uv;
    o.opacity = i.opacity;
    return o;
}

@fragment
fn fs(i: O) -> @location(0) vec4<f32> {
    let c = textureSample(image, image_sampler, i.uv);
    return vec4(c.rgb * c.a, c.a) * i.opacity;
}
