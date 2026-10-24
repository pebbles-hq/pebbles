// Box shadows (02 §8.2, R40): a rounded rect convolved with a Gaussian of standard deviation σ
// (the true Gaussian σ, Flutter `MaskFilter.blur` semantics), evaluated in the rect's LOCAL
// space so it is exact under any affine. prepare/shadows.rs derives the formula; in short, with
// Φ the standard normal CDF and X(y') the half width of the shape's row y':
//
//   value(x, y) = ∫ g_σ(y − y') · [Φ((X(y') − x)/σ) − Φ((−X(y') − x)/σ)] dy'
//
// The straight band (X = half width) is integrated exactly; each corner band is cut into
// CORNER_SLICES slices whose weights are their exact Gaussian masses, with X taken at the slice
// midpoint. Only the part of a corner band within ±4σ of y is sliced (the rest has < 4·10⁻⁵ of
// the mass), so the slices stay narrow next to σ.
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

// Gaussian mass of the row |x'| ≤ half_w seen from x: Φ((half_w − x)/σ) − Φ((−half_w − x)/σ),
// with `s` = 1/(σ·√2).
fn row_mass(x: f32, half_w: f32, s: f32) -> f32 {
    return 0.5 * (erf_as((x + half_w) * s) - erf_as((x - half_w) * s));
}

const CORNER_SLICES: u32 = 12u;

// One corner band y' ∈ [lo, hi] (on one side, |y'| ≥ straight), clipped to ±4σ around y.
fn corner_band(x: f32, y: f32, lo_in: f32, hi_in: f32, shape: vec4<f32>, straight: f32, s: f32) -> f32 {
    let reach = 4.0 * shape.w;
    let lo = max(lo_in, y - reach);
    let hi = min(hi_in, y + reach);
    if (!(hi > lo)) { return 0.0; }
    let r = shape.z;
    let step = (hi - lo) / f32(CORNER_SLICES);
    var e_prev = erf_as((lo - y) * s);
    var sum = 0.0;
    for (var i = 0u; i < CORNER_SLICES; i = i + 1u) {
        let e_next = erf_as((lo + f32(i + 1u) * step - y) * s);
        let mid = lo + (f32(i) + 0.5) * step;
        let into_arc = abs(mid) - straight;
        let half_w = shape.x - r + sqrt(max(r * r - into_arc * into_arc, 0.0));
        sum = sum + 0.5 * (e_next - e_prev) * row_mass(x, half_w, s);
        e_prev = e_next;
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
    let s = 0.70710678 / max(shape.w, 1e-6);
    // The straight band |y'| ≤ hh − r: separable, exact.
    let straight = shape.y - shape.z;
    var value = 0.5 * (erf_as((straight - p.y) * s) - erf_as((-straight - p.y) * s)) * row_mass(p.x, shape.x, s);
    if (shape.z > 0.0) {
        value = value + corner_band(p.x, p.y, straight, shape.y, shape, straight, s);
        value = value + corner_band(p.x, p.y, -shape.y, -straight, shape, straight, s);
    }
    let dp = device_pos(in.clip);
    return finish_color(in.color * (clamp(value, 0.0, 1.0) * (clip_coverage(dp) * clip_mask_coverage(dp))));
}
