// Box shadows (02 §8.2, R40): the Gaussian blur of a rounded rect evaluated in the rect's LOCAL
// space, so it is exact under any affine. x is integrated exactly (erf); y numerically over ±3σ
// (Evan Wallace's rounded-box shadow, MIT). σ = std_dev: the true Gaussian standard deviation
// (Skia `MaskFilter.blur` / Flutter semantics, 02 §8.2).
// The local position is an interpolated varying: affine maps interpolate exactly.

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

// Coverage of one row of the rounded box along x (exact Gaussian integral in x): the row at
// height `y` spans ±curved, shortened by the corner arc (Evan Wallace's rounded-box shadow).
fn row_x(x: f32, y: f32, sigma: f32, corner: f32, half: vec2<f32>) -> f32 {
    let delta = min(half.y - corner - abs(y), 0.0);
    let curved = half.x - corner + sqrt(max(0.0, corner * corner - delta * delta));
    let k = 0.70710678 / sigma;
    return 0.5 * (erf7((x + curved) * k) - erf7((x - curved) * k));
}

const SHADOW_SAMPLES: u32 = 12u;

@fragment
fn fs(in: ShadowOut) -> @location(0) vec4<f32> {
    // params0 = (corner radius (clamped), σ (Gaussian std, local units), 0, 0).
    let corner = in.params0.x;
    let sigma = max(in.params0.y, 1e-6);
    let half = 0.5 * in.size;
    let p = in.local - half;
    // Integrate the row coverage against the Gaussian in y over ±3σ (midpoint rule), clamped to
    // the rows the box occupies.
    let low = p.y - half.y;
    let high = p.y + half.y;
    let start = clamp(-3.0 * sigma, low, high);
    let end = clamp(3.0 * sigma, low, high);
    let step = (end - start) / f32(SHADOW_SAMPLES);
    var y = start + 0.5 * step;
    var value = 0.0;
    let norm = 0.39894228 / sigma; // 1 / (σ √(2π))
    for (var i = 0u; i < SHADOW_SAMPLES; i = i + 1u) {
        let g = norm * exp(-0.5 * y * y / (sigma * sigma));
        value = value + row_x(p.x, p.y - y, sigma, corner, half) * g * step;
        y = y + step;
    }
    return finish_color(in.color * clamp(value, 0.0, 1.0));
}
