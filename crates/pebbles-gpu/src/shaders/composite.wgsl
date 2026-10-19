// Composite a single-sample layer texture into the current target (09 §3.7). No sampler:
// textureLoad with integer texel coordinates (1:1 pixel mapping, WebGL2 texelFetch).
struct CompositeParams {
    bounds: vec4<f32>,
    alpha: f32,
    mix_mode: u32,
    compose: u32,
    _pad: u32,
};
@group(1) @binding(0) var layer_tex: texture_2d<f32>;
@group(1) @binding(1) var<uniform> comp: CompositeParams;

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let corner = vec2<f32>(f32(vi & 1u), f32(vi >> 1u));
    return to_clip(mix(comp.bounds.xy, comp.bounds.zw, corner));
}

fn layer_texel(p: vec2<f32>) -> vec4<f32> {
    return textureLoad(layer_tex, vec2<i32>(floor(p - comp.bounds.xy)), 0);
}

// CompositeOver: premultiplied source-over (BlendState::PREMULTIPLIED_ALPHA_BLENDING). The only
// fragment without `finish_color`: the layer's texels already passed through it when they were
// drawn, so a second decode would double-convert on the GL sRGB path (R13).
// `clip_coverage` (L04) and `clip_mask_coverage` (L08) apply the composited run's clips once.
@fragment
fn fs_over(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let p = device_pos(frag_coord);
    return layer_texel(p) * (comp.alpha * clip_coverage(p) * clip_mask_coverage(p));
}

// CompositeCompose (L21): a Normal-mix layer under a scissor-only clip; the fixed-function
// blend state of 09 §4.3 applies the Porter-Duff operator (coverage is binary there).
@fragment
fn fs_compose(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    return layer_texel(device_pos(frag_coord)) * comp.alpha;
}

// CompositeBackdrop (L23): W3C Compositing and Blending Level 1 in premultiplied gamma space
// (09 §4.5), against a copy of the parent's pixels under the layer bounds; `blend: None`.
// Own code from the W3C text; `mix(B, result, coverage)` is the coverage-weighted composite.
@group(1) @binding(2) var backdrop_tex: texture_2d<f32>;

fn lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.3, 0.59, 0.11));
}

fn clip_color(c_in: vec3<f32>) -> vec3<f32> {
    var c = c_in;
    let l = lum(c);
    let n = min(c.r, min(c.g, c.b));
    let x = max(c.r, max(c.g, c.b));
    if (n < 0.0) {
        c = l + (c - l) * l / max(l - n, 1e-15);
    }
    if (x > 1.0) {
        c = l + (c - l) * (1.0 - l) / max(x - l, 1e-15);
    }
    return c;
}

fn set_lum(c: vec3<f32>, l: f32) -> vec3<f32> {
    return clip_color(c + (l - lum(c)));
}

fn sat(c: vec3<f32>) -> f32 {
    return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
}

fn set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    let mn = min(c.r, min(c.g, c.b));
    let mx = max(c.r, max(c.g, c.b));
    if (mx > mn) {
        return (c - mn) * s / (mx - mn);
    }
    return vec3<f32>(0.0);
}

fn screen3(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return b + s - b * s;
}

fn hard_light3(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return select(screen3(b, 2.0 * s - 1.0), b * 2.0 * s, s <= vec3<f32>(0.5));
}

fn color_dodge1(b: f32, s: f32) -> f32 {
    if (b == 0.0) { return 0.0; }
    if (s >= 1.0) { return 1.0; }
    return min(1.0, b / (1.0 - s));
}

fn color_burn1(b: f32, s: f32) -> f32 {
    if (b >= 1.0) { return 1.0; }
    if (s <= 0.0) { return 0.0; }
    return 1.0 - min(1.0, (1.0 - b) / s);
}

fn soft_light1(b: f32, s: f32) -> f32 {
    if (s <= 0.5) {
        return b - (1.0 - 2.0 * s) * b * (1.0 - b);
    }
    let d = select(sqrt(b), ((16.0 * b - 12.0) * b + 4.0) * b, b <= 0.25);
    return b + (2.0 * s - 1.0) * (d - b);
}

// `B(Cb, Cs)` for `peniko::Mix as u8` (09 §4.5 table).
fn blend_mix(b: vec3<f32>, s: vec3<f32>, mode: u32) -> vec3<f32> {
    switch mode {
        case 1u: { return b * s; }
        case 2u: { return screen3(b, s); }
        case 3u: { return hard_light3(s, b); }
        case 4u: { return min(b, s); }
        case 5u: { return max(b, s); }
        case 6u: { return vec3<f32>(color_dodge1(b.r, s.r), color_dodge1(b.g, s.g), color_dodge1(b.b, s.b)); }
        case 7u: { return vec3<f32>(color_burn1(b.r, s.r), color_burn1(b.g, s.g), color_burn1(b.b, s.b)); }
        case 8u: { return hard_light3(b, s); }
        case 9u: { return vec3<f32>(soft_light1(b.r, s.r), soft_light1(b.g, s.g), soft_light1(b.b, s.b)); }
        case 10u: { return abs(b - s); }
        case 11u: { return b + s - 2.0 * b * s; }
        case 12u: { return set_lum(set_sat(s, sat(b)), lum(b)); }
        case 13u: { return set_lum(set_sat(b, sat(s)), lum(b)); }
        case 14u: { return set_lum(s, lum(b)); }
        case 15u: { return set_lum(b, lum(s)); }
        default: { return s; }
    }
}

// Porter-Duff `(Fa, Fb)` for `peniko::Compose as u8` (09 §4.3).
fn compose_factors(compose: u32, as_: f32, ab: f32) -> vec2<f32> {
    switch compose {
        case 1u: { return vec2<f32>(1.0, 0.0); }
        case 2u: { return vec2<f32>(0.0, 1.0); }
        case 3u: { return vec2<f32>(1.0, 1.0 - as_); }
        case 4u: { return vec2<f32>(1.0 - ab, 1.0); }
        case 5u: { return vec2<f32>(ab, 0.0); }
        case 6u: { return vec2<f32>(0.0, as_); }
        case 7u: { return vec2<f32>(1.0 - ab, 0.0); }
        case 8u: { return vec2<f32>(0.0, 1.0 - as_); }
        case 9u: { return vec2<f32>(ab, 1.0 - as_); }
        case 10u: { return vec2<f32>(1.0 - ab, as_); }
        case 11u: { return vec2<f32>(1.0 - ab, 1.0 - as_); }
        case 12u, 13u: { return vec2<f32>(1.0, 1.0); }
        default: { return vec2<f32>(0.0, 0.0); }
    }
}

fn composite_px(b: vec4<f32>, s: vec4<f32>, mix_mode: u32, compose: u32) -> vec4<f32> {
    let cs = s.rgb / max(s.a, 1e-15);
    let cb = b.rgb / max(b.a, 1e-15);
    let cs2 = mix(cs, blend_mix(cb, cs, mix_mode), b.a);
    let f = compose_factors(compose, s.a, b.a);
    return min(vec4<f32>(1.0), vec4<f32>(s.a * f.x * cs2 + b.a * f.y * cb, s.a * f.x + b.a * f.y));
}

@fragment
fn fs_backdrop(@builtin(position) frag_coord: vec4<f32>) -> @location(0) vec4<f32> {
    let p = device_pos(frag_coord);
    let texel = vec2<i32>(floor(p - comp.bounds.xy));
    let s = textureLoad(layer_tex, texel, 0) * comp.alpha;
    let b = textureLoad(backdrop_tex, texel, 0);
    return mix(b, composite_px(b, s, comp.mix_mode, comp.compose), clip_coverage(p) * clip_mask_coverage(p));
}
