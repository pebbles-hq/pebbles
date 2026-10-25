// Box shadows (02 §8.2, R40): a rounded rect convolved with a Gaussian of standard deviation σ
// (the true Gaussian σ, Flutter `MaskFilter.blur` semantics), evaluated in the rect's LOCAL
// space so it is exact under any affine. prepare/shadows.rs derives the formula; in short, with
// Φ the standard normal CDF and X(y') the half width of the shape's row y':
//
//   value(x, y) = ∫ g_σ(y − y') · [Φ((X(y') − x)/σ) − Φ((−X(y') − x)/σ)] dy'
//
// The straight band (X = half width) is a product of two exact Φ differences. Each corner band
// (M33) comes from the precomputed blur profile T(u, v; ρ) (execute/resources/shadow_profile.rs,
// group 1), read with `textureLoad` and interpolated in code: bilinear in (u, v), linear in
// ρ = r/σ. Corners with ρ above the table (sharp corners on a small blur) fall back to the
// per-pixel integration: CORNER_SLICES slices weighted by their exact Gaussian masses, X at the
// slice midpoint, only within ±4σ of y. The value is even in x and y, so it is evaluated at
// (|x|, |y|).
// The local position is an interpolated varying: affine maps interpolate exactly.

@group(1) @binding(0) var shadow_profile: texture_2d<f32>;

// Must match execute/resources/shadow_profile.rs.
const PROFILE_STEP: f32 = 0.125;
const PROFILE_RHO_STEP: f32 = 0.125;
const PROFILE_RHO_MAX: f32 = 4.0;
const PROFILE_PAD: f32 = 4.0;
const PROFILE_LEVELS: i32 = 33;
const PROFILE_CELL: i32 = 97;
const PROFILE_COLS: i32 = 16;


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
    // Local position relative to the rect's center.
    @location(0) centered: vec2<f32>,
    // (half width, half height, corner radius, σ).
    @location(1) @interpolate(flat) shape: vec4<f32>,
    @location(2) @interpolate(flat) color: vec4<f32>,
    // M33: the occluded hole (centered local x0, y0, x1, y1; empty = none).
    @location(3) @interpolate(flat) hole: vec4<f32>,
};

@vertex
fn vs(in: ShadowIn) -> ShadowOut {
    let k = in.kernel.x;
    let size = in.origin_size.zw;
    // The local quad [−k, size + k]², placed by the linear part (columns (a, b) and (c, d)).
    let local = quad_corner(in.vi) * (size + vec2<f32>(2.0 * k)) - vec2<f32>(k);
    let lin = mat2x2<f32>(in.xform.xy, in.xform.zw);
    var out: ShadowOut;
    out.clip = to_clip(in.origin_size.xy + lin * local);
    out.centered = local - 0.5 * size;
    out.shape = in.params0;
    out.color = in.color;
    out.hole = in.params1;
    return out;
}

// erf after Abramowitz & Stegun, Handbook of Mathematical Functions (1964), 7.1.26: absolute
// error ≤ 1.5e-7; odd extension for negative arguments (same formula as prepare/shadows.rs).
fn erf_as(x: f32) -> f32 {
    let ax = abs(x);
    let t = 1.0 / (1.0 + 0.3275911 * ax);
    let poly = t * (0.254829592 + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    let y = 1.0 - poly * exp(-ax * ax);
    return select(-y, y, x >= 0.0);
}

// Gaussian mass of the row |x'| ≤ half_w seen from x ≥ 0: Φ((half_w − x)/σ) − Φ((−half_w − x)/σ),
// with `s` = 1/(σ·√2). When `wide` (x + half_w ≥ 4σ) the second Φ is below Φ(−4) ≈ 3·10⁻⁵ and is
// dropped (erf((x + half_w)·s) taken as 1), saving one erf.
fn row_mass(x: f32, half_w: f32, s: f32, wide: bool) -> f32 {
    let near = select(erf_as((x + half_w) * s), 1.0, wide);
    return 0.5 * (near - erf_as((x - half_w) * s));
}

const CORNER_SLICES: u32 = 12u;

// One corner band y' ∈ [lo, hi] (on one side, |y'| ≥ straight), clipped to ±4σ around y; x ≥ 0.
fn corner_band(x: f32, y: f32, lo_in: f32, hi_in: f32, shape: vec4<f32>, straight: f32, s: f32) -> f32 {
    let reach = 4.0 * shape.w;
    let lo = max(lo_in, y - reach);
    let hi = min(hi_in, y + reach);
    if (!(hi > lo)) { return 0.0; }
    let r = shape.z;
    let core = shape.x - r;
    // Every row of the band spans [−core, core] ⊇ [x − 4σ, x + 4σ]: row mass 1 (to 6·10⁻⁵).
    if (x + reach <= core) {
        return 0.5 * (erf_as((hi - y) * s) - erf_as((lo - y) * s));
    }
    // No row reaches within 4σ of x: nothing (to 3·10⁻⁵).
    if (x - reach >= shape.x) { return 0.0; }
    let wide = x + core >= reach;
    let step = (hi - lo) / f32(CORNER_SLICES);
    var e_prev = erf_as((lo - y) * s);
    var sum = 0.0;
    for (var i = 0u; i < CORNER_SLICES; i = i + 1u) {
        let e_next = erf_as((lo + f32(i + 1u) * step - y) * s);
        let mid = lo + (f32(i) + 0.5) * step;
        let into_arc = abs(mid) - straight;
        let half_w = core + sqrt(max(r * r - into_arc * into_arc, 0.0));
        sum = sum + 0.5 * (e_next - e_prev) * row_mass(x, half_w, s, wide);
        e_prev = e_next;
    }
    return sum;
}


// Bilinear T(u, v) on level `level` (side `n` texels at cell `cell`).
fn profile_level(level: i32, u: f32, v: f32) -> f32 {
    let n = level + i32(2.0 * PROFILE_PAD / PROFILE_STEP) + 1;
    let cell = vec2<i32>(level % PROFILE_COLS, level / PROFILE_COLS) * PROFILE_CELL;
    let f = clamp((vec2<f32>(u, v) + PROFILE_PAD) / PROFILE_STEP, vec2<f32>(0.0), vec2<f32>(f32(n - 1)));
    let i = min(vec2<i32>(f), vec2<i32>(n - 2));
    let a = f - vec2<f32>(i);
    let o = cell + i;
    let t00 = textureLoad(shadow_profile, o, 0).x;
    let t10 = textureLoad(shadow_profile, o + vec2<i32>(1, 0), 0).x;
    let t01 = textureLoad(shadow_profile, o + vec2<i32>(0, 1), 0).x;
    let t11 = textureLoad(shadow_profile, o + vec2<i32>(1, 1), 0).x;
    return mix(mix(t00, t10, a.x), mix(t01, t11, a.x), a.y);
}

// T(u, v; ρ), linear between the two nearest levels; ρ ≤ PROFILE_RHO_MAX.
fn profile(u: f32, v: f32, rho: f32) -> f32 {
    let f = rho / PROFILE_RHO_STEP;
    let level = min(i32(f), PROFILE_LEVELS - 2);
    return mix(profile_level(level, u, v), profile_level(level + 1, u, v), f - f32(level));
}

// Φ(z) − Φ(z − ρ) with `s` = 1/√2 (the band's mass seen from v = z).
fn band_mass(v: f32, rho: f32) -> f32 {
    return 0.5 * (erf_as(v * 0.70710678) - erf_as((v - rho) * 0.70710678));
}

// One corner band from the profile: v = (row distance past the straight part)/σ. Row masses are
// Φ((X − x)/σ) + Φ((X + x)/σ) − 1, so the band is T(u₊) + T(u₋) − mass with
// u± = (±x − core)/σ; T(u₋) = mass to 3·10⁻⁵ once u₋ ≤ −4.
fn corner_profile(x: f32, v: f32, shape: vec4<f32>, rho: f32) -> f32 {
    if (v <= -PROFILE_PAD || v >= rho + PROFILE_PAD) { return 0.0; }
    let core = shape.x - shape.z;
    var sum = profile((x - core) / shape.w, v, rho);
    let u_far = (-x - core) / shape.w;
    if (u_far > -PROFILE_PAD) {
        sum = sum + profile(u_far, v, rho) - band_mass(v, rho);
    }
    return sum;
}

@fragment
fn fs(in: ShadowOut) -> @location(0) vec4<f32> {
    let shape = in.shape;
    let p = in.centered;
    // M33: covered by the opaque fill drawn next (prepare/plan.rs `occlude_shadows`).
    let h = in.hole;
    if (p.x > h.x && p.x < h.z && p.y > h.y && p.y < h.w) {
        discard;
    }
    let q = abs(p);
    let s = 0.70710678 / max(shape.w, 1e-6);
    // The straight band |y'| ≤ hh − r: separable, exact.
    let straight = shape.y - shape.z;
    let wide = q.x + shape.x >= 4.0 * shape.w;
    var value = 0.5 * (erf_as((straight - q.y) * s) + erf_as((straight + q.y) * s)) * row_mass(q.x, shape.x, s, wide);
    if (shape.z > 0.0) {
        let rho = shape.z / shape.w;
        if (rho <= PROFILE_RHO_MAX) {
            value = value + corner_profile(q.x, (q.y - straight) / shape.w, shape, rho);
            value = value + corner_profile(q.x, (-q.y - straight) / shape.w, shape, rho);
        } else {
            value = value + corner_band(q.x, q.y, straight, shape.y, shape, straight, s);
            value = value + corner_band(q.x, q.y, -shape.y, -straight, shape, straight, s);
        }
    }
    let dp = device_pos(in.clip);
    return finish_color(in.color * (clamp(value, 0.0, 1.0) * (clip_coverage(dp) * clip_mask_coverage(dp))));
}
