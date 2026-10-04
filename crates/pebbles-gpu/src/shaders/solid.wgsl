@group(0) @binding(0) var<uniform> size: vec4<f32>;

struct I {
    @location(0) p: vec2<f32>,
    @location(1) c: vec4<f32>,
}

struct O {
    @builtin(position) p: vec4<f32>,
    @location(0) c: vec4<f32>,
}

@vertex
fn vs(i: I) -> O {
    var o: O;
    o.p = vec4((i.p.x / size.x) * 2.0 - 1.0, 1.0 - (i.p.y / size.y) * 2.0, 0.0, 1.0);
    o.c = i.c;
    return o;
}

@fragment
fn fs(i: O) -> @location(0) vec4<f32> {
    return i.c;
}
