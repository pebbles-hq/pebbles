// Tier A analytic shapes (02 §6.4): one instanced quad per rect / rounded rect / circle with SDF
// coverage from the interpolated device position (never @builtin(position), so it stays correct
// inside offset layer targets).
struct ShapeIn {
    @builtin(vertex_index) vi: u32,
    @location(0) outer: vec4<f32>,
    @location(1) outer_radii: vec4<f32>,
    @location(2) inner: vec4<f32>,
    @location(3) inner_radii: vec4<f32>,
    @location(4) color: vec4<f32>,
};

struct ShapeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) p: vec2<f32>,
    @location(1) @interpolate(flat) outer: vec4<f32>,
    @location(2) @interpolate(flat) outer_radii: vec4<f32>,
    @location(3) @interpolate(flat) inner: vec4<f32>,
    @location(4) @interpolate(flat) inner_radii: vec4<f32>,
    @location(5) @interpolate(flat) color: vec4<f32>,
};

@vertex fn vs_shape(in: ShapeIn) -> ShapeOut {
    // Strip order: (0,0) (1,0) (0,1) (1,1).
    let corner = vec2<f32>(f32(in.vi & 1u), f32((in.vi >> 1u) & 1u));
    let p = mix(in.outer.xy - vec2<f32>(1.0), in.outer.zw + vec2<f32>(1.0), corner); // 1 px AA margin
    var out: ShapeOut;
    out.clip = to_clip(p);
    out.p = p;
    out.outer = in.outer;
    out.outer_radii = in.outer_radii;
    out.inner = in.inner;
    out.inner_radii = in.inner_radii;
    out.color = in.color;
    return out;
}

// Signed distance to a rounded rect; radii = (tl, tr, br, bl).
fn sd_rrect(p: vec2<f32>, rect: vec4<f32>, radii: vec4<f32>) -> f32 {
    let center = (rect.xy + rect.zw) * 0.5;
    let half_size = (rect.zw - rect.xy) * 0.5;
    let d = p - center;
    let r_top = select(radii.x, radii.y, d.x > 0.0);
    let r_bottom = select(radii.w, radii.z, d.x > 0.0);
    let r = select(r_top, r_bottom, d.y > 0.0);
    let q = abs(d) - half_size + vec2<f32>(r);
    return length(max(q, vec2<f32>(0.0))) + min(max(q.x, q.y), 0.0) - r;
}

// Area of the pixel box [p - 0.5, p + 0.5]² inside `rect`: exact for a sharp rect, and an upper
// bound of a rounded rect's area (rounded corners are cut by the SDF term).
fn box_overlap(p: vec2<f32>, rect: vec4<f32>) -> f32 {
    let lo = max(rect.xy, p - vec2<f32>(0.5));
    let hi = min(rect.zw, p + vec2<f32>(0.5));
    let o = clamp(hi - lo, vec2<f32>(0.0), vec2<f32>(1.0));
    return o.x * o.y;
}

// Coverage of the pixel at `p` by a rounded rect (02 §5.1): exact on straight edges and sharp
// corners, SDF-estimated on corner arcs.
fn rrect_coverage(p: vec2<f32>, rect: vec4<f32>, radii: vec4<f32>) -> f32 {
    let sdf = clamp(0.5 - sd_rrect(p, rect, radii), 0.0, 1.0);
    return min(box_overlap(p, rect), sdf);
}

@fragment fn fs_shape(in: ShapeOut) -> @location(0) vec4<f32> {
    var coverage = rrect_coverage(in.p, in.outer, in.outer_radii);
    if (in.inner.z > in.inner.x && in.inner.w > in.inner.y) {
        // A border is the ring between nested shapes: outer minus inner area.
        coverage = max(coverage - rrect_coverage(in.p, in.inner, in.inner_radii), 0.0);
    }
    return finish_color(in.color * coverage);
}
