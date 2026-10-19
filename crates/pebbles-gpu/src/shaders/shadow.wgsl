// Box shadows (02 §8.2, R40): the closed-form Gaussian blur of a rounded rect evaluated in the
// rect's LOCAL space, so it is exact under any affine. Port of vello_sparse_shaders-0.2.0
// `shaders/render.wesl:1099-1160` (Apache-2.0 OR MIT), re-typed from the 02 §8.2 spec. The local
// position is an interpolated varying: affine maps interpolate exactly (no inverse matrix).

struct ShadowIn {
    @builtin(vertex_index) vi: u32,
    @location(0) xform: vec4<f32>,
    @location(1) origin_size: vec4<f32>,
    @location(2) params0: vec4<f32>,
    @location(3) params1: vec4<f32>,
    @location(4) color: vec4<f32>,
    @location(5) kernel: vec4<f32>,
};

struct ShadowOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) size: vec2<f32>,
    @location(2) @interpolate(flat) params0: vec4<f32>,
    @location(3) @interpolate(flat) params1: vec4<f32>,
    @location(4) @interpolate(flat) color: vec4<f32>,
};

@vertex
fn vs(in: ShadowIn) -> ShadowOut {
    let corner = vec2<f32>(f32(in.vi & 1u), f32((in.vi >> 1u) & 1u));
    let k = in.kernel.x;
    let local = mix(vec2<f32>(-k), in.origin_size.zw + vec2<f32>(k), corner);
    let m = mat2x2<f32>(in.xform.xy, in.xform.zw); // columns (a, b) and (c, d)
    var out: ShadowOut;
    out.clip = to_clip(in.origin_size.xy + m * local);
    out.local = local;
    out.size = in.origin_size.zw;
    out.params0 = in.params0;
    out.params1 = in.params1;
    out.color = in.color;
    return out;
}

fn erf7(x: f32) -> f32 {
    let y = clamp(x * 1.1283791671, -100.0, 100.0);
    let yy = y * y;
    let z = y + (0.24295 + (0.03395 + 0.0104 * yy) * yy) * (y * yy);
    return z / sqrt(1.0 + z * z);
}

@fragment
fn fs(in: ShadowOut) -> @location(0) vec4<f32> {
    let exponent = in.params0.x;
    let recip_exponent = in.params0.y;
    let scale = in.params0.z;
    let std_dev_inv = in.params0.w;
    let min_edge = in.params1.x;
    let w = in.params1.y;
    let h = in.params1.z;
    let r1 = in.params1.w;
    let y = in.local.y - 0.5 * in.size.y;
    let y0 = r1 + abs(y) - 0.5 * h;
    let y1 = max(y0, 0.0);
    let x = in.local.x - 0.5 * in.size.x;
    let x0 = r1 + abs(x) - 0.5 * w;
    let x1 = max(x0, 0.0);
    let d_pos = pow(pow(x1, exponent) + pow(y1, exponent), recip_exponent);
    let d_neg = min(max(x0, y0), 0.0);
    let d = d_pos + d_neg - r1;
    let coverage = scale * (erf7(std_dev_inv * (min_edge + d)) - erf7(std_dev_inv * d));
    return finish_color(in.color * coverage);
}
