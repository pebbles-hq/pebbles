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

// Analytic coverage of the active SDF clip at device px `p` (1.0 when the NONE record is bound).
fn clip_coverage(p: vec2<f32>) -> f32 {
    return clamp(0.5 - sd_rrect(p, clip.rect, clip.radii), 0.0, 1.0);
}
