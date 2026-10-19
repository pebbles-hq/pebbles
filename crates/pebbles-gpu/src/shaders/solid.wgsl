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
    o.p = to_clip(i.p);
    o.c = i.c;
    return o;
}

@fragment
fn fs(i: O) -> @location(0) vec4<f32> {
    return finish_color(i.c * clip_coverage(device_pos(i.p)));
}
