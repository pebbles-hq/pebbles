// Gradient brushes (09 §5.5, §6 brush × tier rule): one paint function, three coverage
// variants — Shape (Tier A instances), Mask (Tier B path-mask quads, coverage from the mask
// atlas at group 0 binding 3) and Mesh (Tier C lyon mesh inside the MSAA run). The LUT holds
// premultiplied sRGB-encoded ramps read with textureLoad (no sampler, WebGL2-safe).
//
// The focal (two-point conical) branch follows Skia's SkTwoPointConicalGradient as structured
// in vello_common 0.2.0 (encode.rs / gradient shaders), Copyright the Vello authors,
// Apache-2.0 OR MIT (attribution: 09 L51).
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

fn lut_lookup(t: f32) -> vec4<f32> {
    var te = apply_extend(t, gparams.info.y);
    te = select(0.0, te, te == te);
    let i = gparams.info.z + u32(te * f32(gparams.info.w - 1u));
    return textureLoad(gradient_lut, vec2<i32>(i32(i % 256u), i32(i / 256u)), 0);
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

fn gradient_out(p: vec2<f32>, coverage: f32) -> vec4<f32> {
    return gradient_paint(p) * (coverage * clip_coverage(p) * clip_mask_coverage(p));
}

// ---- Mesh (Tier C): SolidVertex, color = [a; 4] ----
struct MeshOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) alpha: f32,
};

@vertex
fn vs_mesh(@location(0) p: vec2<f32>, @location(1) c: vec4<f32>) -> MeshOut {
    var o: MeshOut;
    o.pos = to_clip(p);
    o.alpha = c.a;
    return o;
}

@fragment
fn fs_mesh(in: MeshOut) -> @location(0) vec4<f32> {
    return finish_color(gradient_out(device_pos(in.pos), in.alpha));
}

// ---- Shape (Tier A): ShapeInstance, color = [a; 4] ----
struct GShapeIn {
    @builtin(vertex_index) vi: u32,
    @location(0) outer: vec4<f32>,
    @location(1) outer_radii: vec4<f32>,
    @location(2) inner: vec4<f32>,
    @location(3) inner_radii: vec4<f32>,
    @location(4) color: vec4<f32>,
};

struct GShapeOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) p: vec2<f32>,
    @location(1) @interpolate(flat) outer: vec4<f32>,
    @location(2) @interpolate(flat) outer_radii: vec4<f32>,
    @location(3) @interpolate(flat) inner: vec4<f32>,
    @location(4) @interpolate(flat) inner_radii: vec4<f32>,
    @location(5) @interpolate(flat) alpha: f32,
};

@vertex
fn vs_shape(in: GShapeIn) -> GShapeOut {
    let corner = vec2<f32>(f32(in.vi & 1u), f32((in.vi >> 1u) & 1u));
    let p = mix(in.outer.xy - vec2<f32>(1.0), in.outer.zw + vec2<f32>(1.0), corner);
    var out: GShapeOut;
    out.clip = to_clip(p);
    out.p = p;
    out.outer = in.outer;
    out.outer_radii = in.outer_radii;
    out.inner = in.inner;
    out.inner_radii = in.inner_radii;
    out.alpha = in.color.a;
    return out;
}

@fragment
fn fs_shape(in: GShapeOut) -> @location(0) vec4<f32> {
    var coverage = rrect_coverage(in.p, in.outer, in.outer_radii);
    if (in.inner.z > in.inner.x && in.inner.w > in.inner.y) {
        coverage = max(coverage - rrect_coverage(in.p, in.inner, in.inner_radii), 0.0);
    }
    return finish_color(gradient_out(device_pos(in.clip), coverage * in.alpha));
}

// ---- Mask (Tier B): QuadInstance over the mask atlas (group 0 binding 3), color = [a; 4] ----
struct GQuadIn {
    @location(0) xform: vec4<f32>,
    @location(1) origin: vec2<f32>,
    @location(2) uv: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(4) layer_kind: vec2<u32>,
};

struct GQuadOut {
    @builtin(position) p: vec4<f32>,
    @location(0) atlas_px: vec2<f32>,
    @location(1) @interpolate(flat) alpha: f32,
    @location(2) @interpolate(flat) layer: u32,
};

@vertex
fn vs_mask(i: GQuadIn, @builtin(vertex_index) vi: u32) -> GQuadOut {
    let c = quad_corner(vi);
    var o: GQuadOut;
    o.p = to_clip(i.origin + c.x * i.xform.xy + c.y * i.xform.zw);
    o.atlas_px = mix(i.uv.xy, i.uv.zw, c);
    o.alpha = i.color.a;
    o.layer = i.layer_kind.x;
    return o;
}

@fragment
fn fs_mask(in: GQuadOut) -> @location(0) vec4<f32> {
    let size = vec2<i32>(textureDimensions(clip_atlas));
    let texel = clamp(vec2<i32>(floor(in.atlas_px)), vec2<i32>(0), size - vec2<i32>(1));
    let layer = min(i32(in.layer), i32(textureNumLayers(clip_atlas)) - 1);
    let m = textureLoad(clip_atlas, texel, layer, 0).r;
    return finish_color(gradient_out(device_pos(in.p), m * in.alpha));
}
