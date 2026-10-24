// Gradient brushes (09 §5.5, §6 brush × tier rule): one paint function, three coverage
// variants — Shape (Tier A instances), Mask (Tier B path-mask quads, coverage from the mask
// atlas at group 0 binding 3) and Mesh (Tier C lyon mesh inside the MSAA run). The LUT holds
// premultiplied sRGB-encoded ramps read with textureLoad (no sampler, WebGL2-safe).
//
// The ramp parameter t is computed from the gradient-space point q = G·p (prepare/gradient.rs
// documents G and p0 per kind). Written from CSS Images Level 3/4 (linear, conic) and the HTML
// Canvas 2D createRadialGradient definition (two circles, largest ω with r(ω) ≥ 0).
struct GradientParams {
    m0: vec4<f32>,
    m1: vec4<f32>,
    info: vec4<u32>,  // kind, extend, lut_start, lut_len
    p0: vec4<f32>,
    p1: vec4<f32>,
};
@group(1) @binding(0) var gradient_lut: texture_2d<f32>;
@group(1) @binding(1) var<uniform> gparams: GradientParams;

fn apply_extend(t: f32, mode: u32) -> f32 {
    if (mode == 1u) { return fract(t); }
    if (mode == 2u) { return abs(t - 2.0 * round(0.5 * t)); }
    return clamp(t, 0.0, 1.0);
}

fn lut_texel(i: u32) -> vec4<f32> {
    let j = gparams.info.z + i;
    return textureLoad(gradient_lut, vec2<i32>(i32(j % 256u), i32(j / 256u)), 0);
}

// Linear between the two nearest ramp texels (two textureLoads, no sampler): the 8-bit LUT
// keeps sub-LSB precision for the §5.8 dither (float evaluation between texels).
fn lut_lookup(t: f32) -> vec4<f32> {
    var te = apply_extend(t, gparams.info.y);
    te = select(0.0, te, te == te);
    let last = gparams.info.w - 1u;
    let f = te * f32(last);
    let i0 = min(u32(f), last);
    let i1 = min(i0 + 1u, last);
    return mix(lut_texel(i0), lut_texel(i1), f - f32(i0));
}

// Conic (sweep) gradient: the angle of q in [0, 2π), measured from +x towards +y (y down, so
// clockwise on screen), rescaled so p0.x maps to 0 and p0.x + 1/p0.y to 1. The center itself
// has no direction and takes angle 0.
fn sweep_t(q: vec2<f32>) -> f32 {
    let tiny = 1.0 / 4096.0;
    var angle = 0.0;
    if (abs(q.x) >= tiny || abs(q.y) >= tiny) {
        angle = atan2(q.y, q.x);
        if (angle < 0.0) { angle = angle + 6.283185307179586; }
    }
    return (angle - gparams.p0.x) * gparams.p0.y;
}

// Radial gradients: returns (t, valid) with valid 1.0 or 0.0.
//
// Concentric (kind 1): t = p0.x + p0.y·|q|.
// Two circles (kinds 2, 3): in gradient space the circle ω is centered at (ω, 0) with radius
// ρ0 + ω·Δ, so q lies on it when A·ω² − 2·B·ω + C = 0 with A = 1 − Δ², B = q.x + ρ0·Δ and
// C = |q|² − ρ0². The roots are ω = (B ± √(B² − A·C)) / A; they are formed as k / A and C / k
// with k = B + sign(B)·√(B² − A·C), which avoids cancellation (and A = 0, the circle touching
// case, leaves the single root C / (2B)). Canvas 2D paints the largest root whose radius is
// non-negative; with none the pixel is transparent.
fn radial_t(q: vec2<f32>, kind: u32) -> vec2<f32> {
    if (kind == 1u) {
        return vec2<f32>(gparams.p0.x + gparams.p0.y * length(q), 1.0);
    }
    let rho0 = gparams.p0.x;
    let delta = gparams.p0.y;
    let a = gparams.p0.z;
    let b = q.x + rho0 * delta;
    let c = dot(q, q) - rho0 * rho0;
    let disc = b * b - a * c;
    if (disc < 0.0) { return vec2<f32>(0.0, 0.0); }
    let k = b + select(-1.0, 1.0, b >= 0.0) * sqrt(disc);
    var best = 0.0;
    var found = false;
    if (k != 0.0) {
        let near = c / k;
        if (rho0 + near * delta >= 0.0) { best = near; found = true; }
    }
    if (a != 0.0) {
        let far = k / a;
        if (rho0 + far * delta >= 0.0 && (!found || far > best)) { best = far; found = true; }
    }
    return vec2<f32>(best, select(0.0, 1.0, found));
}

// The premultiplied paint at device position `p` (kind 5: the LUT-full solid fallback).
fn gradient_paint(p: vec2<f32>) -> vec4<f32> {
    let kind = gparams.info.x;
    if (kind == 5u) { return gparams.p1; }
    let g = vec2<f32>(dot(gparams.m0.xyz, vec3<f32>(p, 1.0)), dot(gparams.m1.xyz, vec3<f32>(p, 1.0)));
    var t = 0.0;
    var valid = 1.0;
    if (kind == 0u) {
        t = g.x + 0.00001;
    } else if (kind == 4u) {
        t = sweep_t(g);
    } else {
        let r = radial_t(g, kind);
        t = r.x;
        valid = r.y;
    }
    return lut_lookup(t) * valid;
}

// 09 §5.8 (L37): ordered 8×8 dither, zero-mean ±0.5 LSB, keyed by the integer DEVICE pixel
// (identical in layer targets and in the golden model); alpha is never dithered.
var<private> BAYER8: array<u32, 64> = array<u32, 64>(
     0u, 32u,  8u, 40u,  2u, 34u, 10u, 42u,   48u, 16u, 56u, 24u, 50u, 18u, 58u, 26u,
    12u, 44u,  4u, 36u, 14u, 46u,  6u, 38u,   60u, 28u, 52u, 20u, 62u, 30u, 54u, 22u,
     3u, 35u, 11u, 43u,  1u, 33u,  9u, 41u,   51u, 19u, 59u, 27u, 49u, 17u, 57u, 25u,
    15u, 47u,  7u, 39u, 13u, 45u,  5u, 37u,   63u, 31u, 55u, 23u, 61u, 29u, 53u, 21u);

fn dither(c: vec4<f32>, p: vec2<f32>) -> vec4<f32> {
    let pix = vec2<u32>(max(floor(p), vec2<f32>(0.0)));
    let b = f32(BAYER8[(pix.y % 8u) * 8u + (pix.x % 8u)]);
    let d = (b - 31.5) / (64.0 * 255.0);
    return vec4<f32>(clamp(c.rgb + vec3<f32>(d), vec3<f32>(0.0), vec3<f32>(c.a)), c.a);
}

// The brush hooks of brush_cover.wgsl (09 §6 brush × tier rule).
fn brush_paint(p: vec2<f32>) -> vec4<f32> {
    return gradient_paint(p);
}

fn brush_post(c: vec4<f32>, p: vec2<f32>) -> vec4<f32> {
    return dither(c, p);
}
