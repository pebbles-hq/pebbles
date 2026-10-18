struct I {
    @location(0) rect: vec4<f32>,
    @location(1) color: vec4<f32>,
    @location(2) params: vec4<f32>,
}

struct O {
    @builtin(position) clip: vec4<f32>,
    @location(0) p: vec2<f32>,
    @location(1) rect: vec4<f32>,
    @location(2) radius: f32,
    @location(3) sigma: f32,
    @location(4) color: vec4<f32>,
}

@vertex
fn vs(i: I, @builtin(vertex_index) vi: u32) -> O {
    let extend = 3.0 * i.params.y;
    let c = quad_corner(vi);
    let corner = vec2<f32>(i.rect.x + c.x * (i.rect.z - i.rect.x), i.rect.y + c.y * (i.rect.w - i.rect.y));
    let p = corner + (c * 2.0 - 1.0) * extend;
    var o: O;
    o.clip = to_clip(p);
    o.p = p;
    o.rect = i.rect;
    o.radius = i.params.x;
    o.sigma = i.params.y;
    o.color = i.color;
    return o;
}

fn erf(x: f32) -> f32 {
    let s = select(-1.0, 1.0, x >= 0.0);
    let a = abs(x);
    let t = 1.0 / (1.0 + 0.3275911 * a);
    let y = 1.0 - (((((1.061405429 * t - 1.453152027) * t) + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * exp(-a * a);
    return s * y;
}

@fragment
fn fs(i: O) -> @location(0) vec4<f32> {
    let center = vec2<f32>((i.rect.x + i.rect.z) * 0.5, (i.rect.y + i.rect.w) * 0.5);
    let half = vec2<f32>((i.rect.z - i.rect.x) * 0.5, (i.rect.w - i.rect.y) * 0.5);
    let q = abs(i.p - center) - half + vec2<f32>(i.radius, i.radius);
    let sdf = length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - i.radius;
    let coverage = 0.5 * (1.0 + erf(-sdf / (i.sigma * 1.41421356)));
    return finish_color(vec4(i.color.rgb * coverage, i.color.a * coverage));
}
