// Projected (perspective) layers (09 §10.7, L65): the layer texture drawn through the 3×3
// homography `h` (texture px → device px). Clip-space positions carry w′, so the GPU
// interpolates UVs perspective-correctly; edges get analytic AA from the device-space quad.
struct ProjectedParams {
    h: array<vec4<f32>, 3>, // rows (xyz used)
    size: vec4<f32>,        // (content w, h, texture w, h)
    corners: array<vec4<f32>, 2>, // device px of the 4 corners: (c0, c1), (c2, c3), strip order
    aa: vec4<f32>,          // x: 1 = analytic edge AA (all corners in front of the camera)
};
@group(1) @binding(0) var layer_tex: texture_2d<f32>;
@group(1) @binding(1) var layer_sampler: sampler;
@group(1) @binding(2) var<uniform> pp: ProjectedParams;

struct ProjOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> ProjOut {
    let c = quad_corner(vi);
    let local = vec3<f32>(c * pp.size.xy, 1.0);
    let x = dot(pp.h[0].xyz, local);
    let y = dot(pp.h[1].xyz, local);
    let w = dot(pp.h[2].xyz, local);
    // to_clip(p) for p = (x, y) / w, multiplied through by w.
    let o = globals.target_origin;
    let s = globals.inv_target_size;
    var out: ProjOut;
    out.pos = vec4<f32>((x - o.x * w) * 2.0 * s.x - w, w - (y - o.y * w) * 2.0 * s.y, 0.0, w);
    out.uv = c * pp.size.xy / pp.size.zw;
    return out;
}

fn edge_distance(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>, inside: vec2<f32>) -> f32 {
    let d = b - a;
    var n = normalize(vec2<f32>(-d.y, d.x));
    if (dot(inside - a, n) < 0.0) { n = -n; }
    return dot(p - a, n);
}

@fragment
fn fs_main(in: ProjOut) -> @location(0) vec4<f32> {
    let p = device_pos(in.pos);
    let c = textureSampleLevel(layer_tex, layer_sampler, in.uv, 0.0);
    var cov = 1.0;
    if (pp.aa.x > 0.5) {
        let c0 = pp.corners[0].xy;
        let c1 = pp.corners[0].zw;
        let c2 = pp.corners[1].xy;
        let c3 = pp.corners[1].zw;
        let mid = (c0 + c1 + c2 + c3) * 0.25;
        // Strip order (0,0) (1,0) (0,1) (1,1): the outline is c0 → c1 → c3 → c2.
        let d = min(min(edge_distance(p, c0, c1, mid), edge_distance(p, c1, c3, mid)),
                    min(edge_distance(p, c3, c2, mid), edge_distance(p, c2, c0, mid)));
        cov = clamp(0.5 + d, 0.0, 1.0);
    }
    // The layer's texels already passed through finish_color when they were drawn.
    return c * (cov * clip_coverage(p) * clip_mask_coverage(p));
}
