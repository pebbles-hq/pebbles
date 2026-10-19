// Instanced image quads (06 §9.4). Texels are premultiplied at upload (02 §3); the shader
// multiplies by the instance opacity and by analytic edge coverage (R31): the quad is inflated
// by 1 device px and each fragment's signed distance to the four parallelogram edges (device
// px) gives the exact area of the unit pixel on the inner side of that edge (`half_plane`) —
// rotated, skewed and mirrored images get antialiased edges, while an axis-aligned image on
// integer pixels stays texel-exact (every center has d ≥ ½ → coverage 1).
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
    // Quad parameter (0..1 inside, beyond 0..1 in the 1-px fringe).
    @location(1) q: vec2<f32>,
    // Device-px distance between the opposite edges: (across u, across v); opacity; LOD (R35).
    @location(2) @interpolate(flat) span: vec4<f32>,
    // |normal| components of the u edges (xy) and the v edges (zw), sorted (max, min).
    @location(3) @interpolate(flat) normals: vec4<f32>,
    // Edge mask (R36): bit 0 u0, 1 u1, 2 v0, 3 v1 — set = antialiased, clear = top-left rule.
    @location(4) @interpolate(flat) edges: u32,
}

// Area of the unit pixel square on the inner side of a line at signed distance `d` (px) from
// its center, for a unit normal with sorted absolute components `a ≥ b` (exact).
fn half_plane(d: f32, a: f32, b: f32) -> f32 {
    let h = 0.5 * (a + b);
    if d >= h { return 1.0; }
    if d <= -h { return 0.0; }
    let m = 0.5 * (a - b);
    if abs(d) <= m { return 0.5 + d / max(a, 1e-6); }
    let t = h - abs(d);
    let corner = t * t / max(2.0 * a * b, 1e-9);
    return select(corner, 1.0 - corner, d > 0.0);
}

fn edge(d: f32, n: vec2<f32>, aa: bool, start: bool) -> f32 {
    if aa {
        return half_plane(d, n.x, n.y);
    }
    return select(select(0.0, 1.0, d > 0.0), select(0.0, 1.0, d >= 0.0), start);
}

fn sorted_abs(n: vec2<f32>) -> vec2<f32> {
    let v = abs(n) / max(length(n), 1e-9);
    return vec2<f32>(max(v.x, v.y), min(v.x, v.y));
}

@vertex
fn vs(i: QuadIn, @builtin(vertex_index) vi: u32) -> QuadOut {
    let c = quad_corner(vi);
    let x = i.xform.xy;
    let y = i.xform.zw;
    let det = abs(x.x * y.y - x.y * y.x);
    // Height of the parallelogram across each pair of edges.
    let hx = det / max(length(y), 1e-9);
    let hy = det / max(length(x), 1e-9);
    // Inflate by 1 device px on every side.
    let d = vec2<f32>(1.0 / max(hx, 1e-9), 1.0 / max(hy, 1e-9));
    let q = c * (vec2<f32>(1.0) + 2.0 * d) - d;
    var o: QuadOut;
    o.p = to_clip(i.origin + q.x * x + q.y * y);
    o.uv = mix(i.uv.xy, i.uv.zw, q);
    o.q = q;
    o.span = vec4<f32>(hx, hy, i.color.a, i.color.x);
    // u edges run along y (normal ⟂ y); v edges along x.
    o.normals = vec4<f32>(sorted_abs(vec2<f32>(-y.y, y.x)), sorted_abs(vec2<f32>(-x.y, x.x)));
    o.edges = i.layer_kind.x;
    return o;
}

@fragment
fn fs(i: QuadOut) -> @location(0) vec4<f32> {
    // The LOD comes from the CPU (R35), never from derivatives.
    let t = textureSampleLevel(image, image_sampler, i.uv, i.span.w);
    let du = vec2<f32>(i.q.x, 1.0 - i.q.x) * i.span.x;
    let dv = vec2<f32>(i.q.y, 1.0 - i.q.y) * i.span.y;
    let nu = i.normals.xy;
    let nv = i.normals.zw;
    // Internal (unmasked) edges of a nine-slice tile without seams: the top-left rule includes
    // a pixel center on the start edge and excludes one on the end edge.
    let e = edge(du.x, nu, (i.edges & 1u) != 0u, true) * edge(du.y, nu, (i.edges & 2u) != 0u, false)
        * edge(dv.x, nv, (i.edges & 4u) != 0u, true) * edge(dv.y, nv, (i.edges & 8u) != 0u, false);
    return finish_color(t * (i.span.z * e));
}
