@group(1) @binding(0) var atlas: texture_2d<f32>;
@group(1) @binding(1) var atlas_sampler: sampler;

struct I {
    @location(0) p: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) c: vec4<f32>,
}

struct O {
    @builtin(position) p: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) c: vec4<f32>,
}

@vertex
fn vs(i: I) -> O {
    var o: O;
    o.p = to_clip(i.p);
    o.uv = i.uv;
    o.c = i.c;
    return o;
}

@fragment
fn fs_mask(i: O) -> @location(0) vec4<f32> {
    let coverage = textureSample(atlas, atlas_sampler, i.uv).r;
    return vec4(i.c.rgb * coverage, i.c.a * coverage);
}

@fragment
fn fs_color(i: O) -> @location(0) vec4<f32> {
    let c = textureSample(atlas, atlas_sampler, i.uv);
    return vec4(c.rgb * c.a, c.a);
}
