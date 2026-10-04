@group(0) @binding(0) var<uniform> size: vec4<f32>;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

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
    o.p = vec4((i.p.x / size.x) * 2.0 - 1.0, 1.0 - (i.p.y / size.y) * 2.0, 0.0, 1.0);
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
