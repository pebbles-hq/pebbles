// Instanced glyph quads (06 §9.4, 10 §5.9). `uv` carries atlas TEXEL coordinates; the
// fragment filters in texel space (`atlas_bilinear`), so atlas growth (M01) never invalidates instances.
@group(1) @binding(0) var mask_atlas: texture_2d_array<f32>;
@group(1) @binding(1) var color_atlas: texture_2d_array<f32>;
@group(1) @binding(2) var atlas_sampler: sampler;

struct QuadIn {
    @location(0) xform: vec4<f32>,
    @location(1) origin: vec2<f32>,
    @location(2) uv: vec4<f32>,
    @location(3) color: vec4<f32>,
    @location(4) layer_kind: vec2<u32>,
}

struct QuadOut {
    @builtin(position) p: vec4<f32>,
    @location(0) atlas_px: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) layer_kind: vec2<u32>,
}

@vertex
fn vs(i: QuadIn, @builtin(vertex_index) vi: u32) -> QuadOut {
    let c = quad_corner(vi);
    let p = i.origin + c.x * i.xform.xy + c.y * i.xform.zw;
    var o: QuadOut;
    o.p = to_clip(p);
    o.atlas_px = mix(i.uv.xy, i.uv.zw, c);
    o.color = i.color;
    o.layer_kind = i.layer_kind;
    return o;
}

// Bilinear (clamp-to-edge) on one atlas layer from four `textureLoad`s, weights on the 1/256
// grid (DR-21, as `sample_level_portable`): the hardware filter's sub-texel snapping is
// implementation-defined — WARP pulled 1/256 of the zero gutter into texel-centered samples,
// darkening every untransformed glyph and path mask by one LSB.
fn atlas_bilinear(t: texture_2d_array<f32>, px: vec2<f32>, layer: u32) -> vec4<f32> {
    let size = vec2<i32>(textureDimensions(t));
    let q = px - vec2<f32>(0.5);
    let base = floor(q);
    let f = floor((q - base) * 256.0 + vec2<f32>(0.5)) / 256.0;
    let i = vec2<i32>(base);
    let hi = size - vec2<i32>(1);
    let x0 = clamp(i.x, 0, hi.x);
    let x1 = clamp(i.x + 1, 0, hi.x);
    let y0 = clamp(i.y, 0, hi.y);
    let y1 = clamp(i.y + 1, 0, hi.y);
    let a = textureLoad(t, vec2<i32>(x0, y0), layer, 0);
    let b = textureLoad(t, vec2<i32>(x1, y0), layer, 0);
    let c = textureLoad(t, vec2<i32>(x0, y1), layer, 0);
    let d = textureLoad(t, vec2<i32>(x1, y1), layer, 0);
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// Grayscale text contrast + gamma correction, as GPUI `apply_contrast_and_gamma_correction`
// (adapted from microsoft/terminal dwrite.hlsl, MIT). Coverage is blended in the encoded space;
// this curve compensates (light-on-dark gets the enhanced contrast). CPU mirror:
// `globals::text_coverage`. `color` is the straight sRGB run color.
fn text_coverage(a0: f32, color: vec3<f32>) -> f32 {
    let b = dot(color, vec3<f32>(0.30, 0.59, 0.11));
    let k = globals.text_contrast * saturate(4.0 * (0.75 - b));
    let a = a0 * (k + 1.0) / (a0 * k + 1.0);
    let g = globals.text_gamma;
    let correction = (g.x * b + g.y) * a + (g.z * b + g.w);
    return saturate(a + a * (1.0 - a) * correction);
}

@fragment
fn fs_main(in: QuadOut) -> @location(0) vec4<f32> {
    // The layer belongs to ONE kind; the other kind's (discarded) sample reads layer 0, which
    // always exists. Not `min(layer, textureNumLayers(..) - 1)`: naga's HLSL backend returns the
    // mip count for arrayed textures (1 here), which pinned every glyph to layer 0 on DX12.
    let is_color = in.layer_kind.y == 1u;
    let mask_layer = select(in.layer_kind.x, 0u, is_color);
    let color_layer = select(0u, in.layer_kind.x, is_color);
    // Both samples in uniform control flow, then select (W5).
    let straight = select(vec3<f32>(0.0), in.color.rgb / in.color.a, in.color.a > 0.0);
    let raw = atlas_bilinear(mask_atlas, in.atlas_px, mask_layer).r;
    // Kind 4 = a text glyph mask (`KIND_TEXT_MASK`); kind 0 path masks keep exact coverage.
    let m = select(raw, text_coverage(raw, straight), in.layer_kind.y == 4u);
    let c = atlas_bilinear(color_atlas, in.atlas_px, color_layer);
    let cc = (clip_coverage(device_pos(in.p)) * clip_mask_coverage(device_pos(in.p)));
    return finish_color(select(in.color * m, c * in.color.a, is_color) * cc);
}
