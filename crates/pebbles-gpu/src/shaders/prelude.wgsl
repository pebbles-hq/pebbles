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

// Identity on the contract path. On a GL sRGB-only surface, convert the premultiplied encoded
// color to premultiplied linear, so the hardware's linear→sRGB store reproduces the encoded value
// for opaque pixels (02 §3.8, R13).
fn finish_color(premul: vec4<f32>) -> vec4<f32> {
    if ((globals.color_flags & COLOR_FLAG_DECODE_SRGB_OUTPUT) == 0u) {
        return premul;
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
    let layer = clamp(i32(mask_clip.atlas.z), 0, i32(textureNumLayers(clip_atlas)) - 1);
    let v = textureLoad(clip_atlas, texel, layer, 0).r;
    return select(1.0, select(0.0, v, inside), mask_clip.atlas.w > 0.5);
}
