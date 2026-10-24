// Layer effects (09 §10.2–§10.6, L61/L63/L64): one full-target triangle per step; every step
// reads its source with textureLoad (texel-exact, no sampler) and writes an Rgba8Unorm pool
// texture, so each pass quantizes like the CPU kernel (raster/blur.rs) and the golden model —
// rounded half up in the shader (`store_unorm8`), not by the backend's float→unorm8 store.
// Only the content rect `src.xy` / `dst.xy` is meaningful; outside it reads as transparent
// (Decal) or the nearest content texel (Clamp).
struct EffectParams {
    info: vec4<u32>,    // (unused, radius, clamp edges, axis 0 = x / 1 = y)
    size: vec4<f32>,    // (src content w, h, dst content w, h)
    weights: array<vec4<f32>, 7>, // Gaussian taps w(0..=r), r ≤ 6, packed 4 per vec4
    matrix: array<vec4<f32>, 5>,  // color matrix rows r′ g′ b′ a′ (4 + 4 + 4 + 4 + 4 = 20 floats)
};
@group(1) @binding(0) var src_tex: texture_2d<f32>;
@group(1) @binding(1) var<uniform> ep: EffectParams;

@vertex
fn vs_full(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    return vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
}

fn src_at(p: vec2<i32>) -> vec4<f32> {
    let size = vec2<i32>(ep.size.xy);
    let inside = all(p >= vec2<i32>(0)) && all(p < size);
    if (ep.info.z == 1u) {
        return textureLoad(src_tex, clamp(p, vec2<i32>(0), size - vec2<i32>(1)), 0);
    }
    let c = textureLoad(src_tex, clamp(p, vec2<i32>(0), size - vec2<i32>(1)), 0);
    return select(vec4<f32>(0.0), c, inside);
}

// Edge-clamped read (downsample / upsample always clamp to the content, like raster/blur.rs).
fn src_clamped(p: vec2<i32>) -> vec4<f32> {
    let size = vec2<i32>(ep.size.xy);
    return textureLoad(src_tex, clamp(p, vec2<i32>(0), size - vec2<i32>(1)), 0);
}

fn weight(i: u32) -> f32 {
    return ep.weights[i / 4u][i % 4u];
}

fn axis() -> vec2<i32> {
    return select(vec2<i32>(1, 0), vec2<i32>(0, 1), ep.info.w == 1u);
}

@fragment
fn fs_down(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(floor(frag.xy)) * 2;
    let s = src_clamped(p) + src_clamped(p + vec2<i32>(1, 0)) + src_clamped(p + vec2<i32>(0, 1)) + src_clamped(p + vec2<i32>(1, 1));
    return store_unorm8(s * 0.25);
}

@fragment
fn fs_blur(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(floor(frag.xy));
    let d = axis();
    let r = i32(ep.info.y);
    var acc = src_at(p) * weight(0u);
    for (var i = 1; i <= r; i = i + 1) {
        acc = acc + (src_at(p + d * i) + src_at(p - d * i)) * weight(u32(i));
    }
    return store_unorm8(acc);
}

@fragment
fn fs_up(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = (floor(frag.xy) + vec2<f32>(0.5)) / 2.0 - vec2<f32>(0.5);
    let f0 = floor(uv);
    let fr = uv - f0;
    let i = vec2<i32>(f0);
    let top = mix(src_clamped(i), src_clamped(i + vec2<i32>(1, 0)), fr.x);
    let bot = mix(src_clamped(i + vec2<i32>(0, 1)), src_clamped(i + vec2<i32>(1, 1)), fr.x);
    return store_unorm8(mix(top, bot, fr.y));
}

// 09 §10.5: unpremultiply, M · (r, g, b, a, 1), clamp, premultiply.
@fragment
fn fs_matrix(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let s = src_at(vec2<i32>(floor(frag.xy)));
    let c = select(vec3<f32>(0.0), s.rgb / s.a, s.a > 0.0);
    let x = vec4<f32>(c, s.a);
    var m: array<f32, 20>;
    for (var k = 0u; k < 20u; k = k + 1u) {
        m[k] = ep.matrix[k / 4u][k % 4u];
    }
    var v: vec4<f32>;
    for (var row = 0u; row < 4u; row = row + 1u) {
        let b = row * 5u;
        v[row] = clamp(m[b] * x.r + m[b + 1u] * x.g + m[b + 2u] * x.b + m[b + 3u] * x.a + m[b + 4u], 0.0, 1.0);
    }
    return store_unorm8(vec4<f32>(v.rgb * v.a, v.a));
}

// 09 §10.6: per-channel max / min over the axis window (Decal: outside counts as transparent).
@fragment
fn fs_dilate(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(floor(frag.xy));
    let d = axis();
    var acc = src_at(p);
    for (var i = 1; i <= i32(ep.info.y); i = i + 1) {
        acc = max(acc, max(src_at(p + d * i), src_at(p - d * i)));
    }
    return acc;
}

@fragment
fn fs_erode(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(floor(frag.xy));
    let d = axis();
    var acc = src_at(p);
    for (var i = 1; i <= i32(ep.info.y); i = i + 1) {
        acc = min(acc, min(src_at(p + d * i), src_at(p - d * i)));
    }
    return acc;
}
