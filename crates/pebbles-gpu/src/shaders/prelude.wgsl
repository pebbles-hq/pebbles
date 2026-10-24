// prelude.wgsl: prepended to every pipeline's WGSL by concat!(include_str!(..)).
// Mirrors `Globals` in execute/resources/globals.rs (owner 10 §2.6; 32 bytes). Keep field order and padding identical.
struct Globals {
    target_size: vec2<f32>,
    target_origin: vec2<f32>,
    inv_target_size: vec2<f32>,
    color_flags: u32,
    _pad0: u32,
};
const COLOR_FLAG_DECODE_SRGB_OUTPUT: u32 = 1u;

@group(0) @binding(0) var<uniform> globals: Globals;

// The active SDF clip (09 §2.4, L04): group 0, binding 1, dynamic offset. Record 0 is "no clip".
struct ClipParams {
    rect: vec4<f32>,   // device px: x0, y0, x1, y1
    radii: vec4<f32>,  // device px: top-left, top-right, bottom-right, bottom-left
};
@group(0) @binding(1) var<uniform> clip: ClipParams;

// The active mask clip (09 L08): a coverage-atlas region over a device rect. atlas.w == 0 → none.
struct MaskClipParams {
    rect: vec4<f32>,   // device px: x0, y0, width, height
    atlas: vec4<f32>,  // atlas texel x, y, layer, enabled
};
@group(0) @binding(2) var<uniform> mask_clip: MaskClipParams;
@group(0) @binding(3) var clip_atlas: texture_2d_array<f32>;
@group(0) @binding(4) var gpu_mask: texture_2d<f32>;

// Signed distance from p to a rounded rect, negative inside. y points down.
fn sd_rrect(p: vec2<f32>, rect: vec4<f32>, radii: vec4<f32>) -> f32 {
    let center = (rect.xy + rect.zw) * 0.5;
    let half_size = (rect.zw - rect.xy) * 0.5;
    let q0 = p - center;
    let r_top = select(radii.x, radii.y, q0.x > 0.0);
    let r_bottom = select(radii.w, radii.z, q0.x > 0.0);
    let r = select(r_bottom, r_top, q0.y < 0.0);
    let q = abs(q0) - half_size + vec2<f32>(r, r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0, 0.0))) - r;
}

fn srgb_to_linear(c: f32) -> f32 {
    return select(pow((c + 0.055) / 1.055, 2.4), c / 12.92, c <= 0.04045);
}

// Round a fragment output to the 8-bit UNORM grid, half up, before the fixed-function blend
// (DR-21). The float→unorm8 conversion of an exact tie (k + 0.5)/255 — e.g. any opaque color
// at coverage or opacity 0.5 — is implementation-defined: lavapipe and WARP round 127.5 down
// or up differently, Metal blends unconverted floats. Pre-rounded source color and alpha make
// the blend `s + d·(255 − a)/255` land at least 1/510 LSB away from any tie (d·(255 − a) is
// never an odd multiple of 255/2), so every backend stores the same byte.
fn store_unorm8(c: vec4<f32>) -> vec4<f32> {
    return floor(clamp(c, vec4<f32>(0.0), vec4<f32>(1.0)) * 255.0 + 0.5) / 255.0;
}

// `store_unorm8` on the contract path; unchanged on a GL sRGB surface (R13), whose store
// encodes linear values and has no 8-bit grid in this space.
fn store_target(c: vec4<f32>) -> vec4<f32> {
    if ((globals.color_flags & COLOR_FLAG_DECODE_SRGB_OUTPUT) == 0u) {
        return store_unorm8(c);
    }
    return c;
}

// Identity on the contract path (up to `store_unorm8`). On a GL sRGB-only surface, convert the premultiplied encoded
// color to premultiplied linear, so the hardware's linear→sRGB store reproduces the encoded value
// for opaque pixels (02 §3.8, R13).
//
// On the contract path the output is pre-rounded to the 8-bit grid (`store_unorm8`).
fn finish_color(premul: vec4<f32>) -> vec4<f32> {
    if ((globals.color_flags & COLOR_FLAG_DECODE_SRGB_OUTPUT) == 0u) {
        return store_unorm8(premul);
    }
    if (premul.a <= 0.0) {
        return vec4<f32>(0.0);
    }
    let straight = premul.rgb / premul.a;
    let lin = vec3<f32>(srgb_to_linear(straight.r), srgb_to_linear(straight.g), srgb_to_linear(straight.b));
    return vec4<f32>(lin * premul.a, premul.a);
}

// Device px (y down) -> clip space for the current pass target (10 §2.6 owns this function).
fn to_clip(p: vec2<f32>) -> vec4<f32> {
    let q = (p - globals.target_origin) * globals.inv_target_size;
    return vec4<f32>(q.x * 2.0 - 1.0, 1.0 - q.y * 2.0, 0.0, 1.0);
}

// Unit-quad corner for instanced 4-vertex triangle strips: 0 (0,0), 1 (1,0), 2 (0,1), 3 (1,1).
fn quad_corner(vertex_index: u32) -> vec2<f32> {
    return vec2<f32>(f32(vertex_index & 1u), f32((vertex_index >> 1u) & 1u));
}

// Fragment @builtin(position) -> device px of the frame (adds the pass target's origin).
fn device_pos(frag_coord: vec4<f32>) -> vec2<f32> {
    return frag_coord.xy + globals.target_origin;
}

// Area of the pixel box [p - 0.5, p + 0.5]² inside `rect`: exact for a sharp rect, and an upper
// bound of a rounded rect's area (rounded corners are cut by the SDF term).
fn box_overlap(p: vec2<f32>, rect: vec4<f32>) -> f32 {
    let lo = max(rect.xy, p - vec2<f32>(0.5));
    let hi = min(rect.zw, p + vec2<f32>(0.5));
    let o = clamp(hi - lo, vec2<f32>(0.0), vec2<f32>(1.0));
    return o.x * o.y;
}

// Whether the pixel at `p` lies within one pixel of a corner arc of `rect` (radii > 0).
fn near_arc(p: vec2<f32>, rect: vec4<f32>, radii: vec4<f32>) -> bool {
    let center = (rect.xy + rect.zw) * 0.5;
    let half_size = (rect.zw - rect.xy) * 0.5;
    let d = p - center;
    let r_top = select(radii.x, radii.y, d.x > 0.0);
    let r_bottom = select(radii.w, radii.z, d.x > 0.0);
    let r = select(r_top, r_bottom, d.y > 0.0);
    let q = abs(d) - half_size + vec2<f32>(r);
    return r > 0.0 && q.x > -1.0 && q.y > -1.0;
}

// Coverage of the pixel at `p` by a rounded rect (02 §5.1): exact on straight edges and sharp
// corners (box overlap); near a corner arc, the SDF box-filtered over 8×8 subsamples (≤ 2 LSB,
// 02 §5.2 Tier A curves) — a single SDF sample per pixel misjudges arc areas by up to ~10 LSB.
fn rrect_coverage(p: vec2<f32>, rect: vec4<f32>, radii: vec4<f32>) -> f32 {
    let box = box_overlap(p, rect);
    // A sharp rect (all radii 0): the box overlap is the exact area.
    if (max(max(radii.x, radii.y), max(radii.z, radii.w)) <= 0.0) {
        return box;
    }
    if (!near_arc(p, rect, radii)) {
        return min(box, clamp(0.5 - sd_rrect(p, rect, radii), 0.0, 1.0));
    }
    var sum = 0.0;
    for (var j = 0; j < 8; j = j + 1) {
        for (var i = 0; i < 8; i = i + 1) {
            let s = p + (vec2<f32>(f32(i), f32(j)) + vec2<f32>(0.5)) / 8.0 - vec2<f32>(0.5);
            // Each subsample is a 1/8-px box: its own linear ramp over ±1/16 px.
            sum = sum + clamp(0.5 - 8.0 * sd_rrect(s, rect, radii), 0.0, 1.0);
        }
    }
    return min(box, sum / 64.0);
}

// Coverage of the active SDF clip at device px `p` (1.0 when the NONE record is bound): the
// same exact-area rounded-rect coverage as Tier A fills (R10), so a clip edge matches a fill edge.
fn clip_coverage(p: vec2<f32>) -> f32 {
    return rrect_coverage(p, clip.rect, clip.radii);
}

// Coverage of the active mask clip at device px `p` (09 L08): the atlas texel under the pixel
// (texel-exact, `textureLoad`); 0 outside the clip rect; 1 when no mask clip is active.
fn clip_mask_coverage(p: vec2<f32>) -> f32 {
    let local = floor(p - mask_clip.rect.xy);
    let inside = all(local >= vec2<f32>(0.0)) && all(local < mask_clip.rect.zw);
    let size = vec2<i32>(textureDimensions(clip_atlas));
    let texel = clamp(vec2<i32>(mask_clip.atlas.xy + local), vec2<i32>(0), size - vec2<i32>(1));
    // No `textureNumLayers` clamp (see brush_cover.wgsl `fs_mask`): the record's layer is a
    // resident slot's, and the NONE record's 0 is always valid.
    let layer = max(i32(mask_clip.atlas.z), 0);
    let v = textureLoad(clip_atlas, texel, layer, 0).r;
    // atlas.w == 2: a GPU mask (L09) at (0, 0) of the texture bound at binding 4.
    let gsize = vec2<i32>(textureDimensions(gpu_mask));
    let g = textureLoad(gpu_mask, clamp(vec2<i32>(local), vec2<i32>(0), gsize - vec2<i32>(1)), 0).r;
    let m = select(v, g, mask_clip.atlas.w > 1.5);
    return select(1.0, select(0.0, m, inside), mask_clip.atlas.w > 0.5);
}

// ---- Portable filtering (DR-21) ----
// Images, image brushes and textured meshes are filtered in code from `textureLoad` texels,
// never by the sampler: hardware bilinear weights carry an implementation-defined sub-texel
// precision and arithmetic (WARP filters 8-bit texels in reduced precision; Vulkan only
// guarantees 4 sub-texel bits), and nearest picks at exact texel boundaries differ. `mode`
// is `SamplerKey::mode`: bit 0 linear, bit 1 linear between levels, bits 2–3 / 4–5 the
// u / v address mode (0 clamp-to-edge, 1 repeat, 2 mirror-repeat).

fn wrap_texel(i: i32, n: i32, address: u32) -> i32 {
    if (address == 1u) {
        return ((i % n) + n) % n;
    }
    if (address == 2u) {
        let p = 2 * n;
        let m = ((i % p) + p) % p;
        return select(m, p - 1 - m, m >= n);
    }
    return clamp(i, 0, n - 1);
}

fn load_wrapped(t: texture_2d<f32>, c: vec2<i32>, size: vec2<i32>, level: i32, mode: u32) -> vec4<f32> {
    let x = wrap_texel(c.x, size.x, (mode >> 2u) & 3u);
    let y = wrap_texel(c.y, size.y, (mode >> 4u) & 3u);
    return textureLoad(t, vec2<i32>(x, y), level);
}

// One level: nearest = the texel containing `uv`; linear = bilinear around the texel centers,
// with weights on the 1/256 grid (the 8 sub-texel bits D3D and most Vulkan drivers use).
fn sample_level_portable(t: texture_2d<f32>, uv: vec2<f32>, level: i32, mode: u32) -> vec4<f32> {
    let size = vec2<i32>(textureDimensions(t, level));
    // Bounded so the i32 conversions below stay exact (repeat/mirror far from the image).
    let p = clamp(uv * vec2<f32>(size), vec2<f32>(-8388608.0), vec2<f32>(8388608.0));
    if ((mode & 1u) == 0u) {
        // Snapped to 1/65536 texel first: a pixel center exactly on a texel boundary (e.g. the
        // diagonal of a 45°-rotated brush) must not pick its texel by the last-bit rounding
        // of the backend's (possibly fused) multiply-adds.
        return load_wrapped(t, vec2<i32>(floor(floor(p * 65536.0 + vec2<f32>(0.5)) / 65536.0)), size, level, mode);
    }
    let q = p - vec2<f32>(0.5);
    let base = floor(q);
    let f = floor((q - base) * 256.0 + vec2<f32>(0.5)) / 256.0;
    let i = vec2<i32>(base);
    let a = load_wrapped(t, i, size, level, mode);
    let b = load_wrapped(t, i + vec2<i32>(1, 0), size, level, mode);
    let c = load_wrapped(t, i + vec2<i32>(0, 1), size, level, mode);
    let d = load_wrapped(t, i + vec2<i32>(1, 1), size, level, mode);
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// `textureSampleLevel` semantics at an explicit LOD, clamped to `0..=top` (the last level,
// passed in: `textureNumLevels` does not exist on GLSL ES 3.0); nearest level = round half up.
fn sample_portable(t: texture_2d<f32>, uv: vec2<f32>, lod: f32, top: f32, mode: u32) -> vec4<f32> {
    let l = clamp(lod, 0.0, top);
    if ((mode & 2u) == 0u) {
        return sample_level_portable(t, uv, i32(floor(l + 0.5)), mode);
    }
    let l0 = floor(l);
    let lo = sample_level_portable(t, uv, i32(l0), mode);
    let hi = sample_level_portable(t, uv, i32(min(l0 + 1.0, top)), mode);
    return mix(lo, hi, l - l0);
}
