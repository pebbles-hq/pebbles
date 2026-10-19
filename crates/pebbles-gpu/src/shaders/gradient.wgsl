// Gradient brushes (09 §5.5, §6 brush × tier rule): one paint function, three coverage
// variants — Shape (Tier A instances), Mask (Tier B path-mask quads, coverage from the mask
// atlas at group 0 binding 3) and Mesh (Tier C lyon mesh inside the MSAA run). The LUT holds
// premultiplied sRGB-encoded ramps read with textureLoad (no sampler, WebGL2-safe).
//
// Ported from vello_sparse_shaders 0.2.0 / vello_common 0.2.0 (the focal branch of gradient
// evaluation), Copyright the Vello Authors, Apache-2.0 OR MIT (used under Apache-2.0); it
// follows Skia's SkTwoPointConicalGradient (BSD-3-Clause). See NOTICE.
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
// keeps sub-LSB precision for the §5.8 dither, like Skia's float gradient evaluation.
fn lut_lookup(t: f32) -> vec4<f32> {
    var te = apply_extend(t, gparams.info.y);
    te = select(0.0, te, te == te);
    let last = gparams.info.w - 1u;
    let f = te * f32(last);
    let i0 = min(u32(f), last);
    let i1 = min(i0 + 1u, last);
    return mix(lut_texel(i0), lut_texel(i1), f - f32(i0));
}

fn sweep_t(g_in: vec2<f32>) -> f32 {
    let g = select(g_in, vec2<f32>(0.0, 0.0), abs(g_in) < vec2<f32>(1.0 / 4096.0, 1.0 / 4096.0));
    var a = 0.0;
    if (g.x != 0.0 || g.y != 0.0) { a = atan2(g.y, g.x); }
    if (a < 0.0) { a = a + 6.283185307179586; }
    return (a - gparams.p0.x) * gparams.p0.y;
}

fn radial_t(g: vec2<f32>, kind: u32) -> vec2<f32> {
    if (kind == 1u) {
        return vec2<f32>(gparams.p0.x + gparams.p0.y * length(g), 1.0);
    }
    if (kind == 2u) {
        let q = gparams.p0.x - g.y * g.y;
        if (q < 0.0) { return vec2<f32>(0.0, 0.0); }
        return vec2<f32>(g.x + sqrt(q), 1.0);
    }
    let fp0 = gparams.p0.x;
    let fp1 = gparams.p0.y;
    let fr1 = gparams.p0.z;
    let focal_x = gparams.p0.w;
    let swapped = gparams.p1.x != 0.0;
    let tol = 1.0 / 4096.0;
    let on_circle = abs(1.0 - fr1) <= tol;
    let well_behaved = !on_circle && fr1 > 1.0;
    let negate = (1.0 - focal_x) < 0.0;
    var t = 0.0;
    var ok = true;
    if (on_circle) {
        t = g.x + g.y * g.y / g.x;
        ok = t >= 0.0 && g.x != 0.0;
    } else if (well_behaved) {
        t = length(g) - g.x * fp0;
    } else {
        let disc = g.x * g.x - g.y * g.y;
        let root = sqrt(max(disc, 0.0));
        t = select(root, -root, swapped || negate) - g.x * fp0;
        ok = disc >= 0.0 && t >= 0.0;
    }
    if (ok) {
        if (negate) { t = -t; }
        if (abs(focal_x) > tol) { t = t + fp1; }
        if (swapped) { t = 1.0 - t; }
    }
    return vec2<f32>(t, select(0.0, 1.0, ok));
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
