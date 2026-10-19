// Instanced glyph quads (06 §9.4, 10 §5.9). `uv` carries atlas TEXEL coordinates; the
// fragment normalizes by the texture size, so atlas growth (M01) never invalidates instances.
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

@fragment
fn fs_main(in: QuadOut) -> @location(0) vec4<f32> {
    let mask_uv = in.atlas_px / vec2<f32>(textureDimensions(mask_atlas));
    let color_uv = in.atlas_px / vec2<f32>(textureDimensions(color_atlas));
    // The layer belongs to ONE kind; clamp for the other so its (discarded) sample stays in range.
    let mask_layer = min(in.layer_kind.x, textureNumLayers(mask_atlas) - 1u);
    let color_layer = min(in.layer_kind.x, textureNumLayers(color_atlas) - 1u);
    // Both samples in uniform control flow, then select (W5).
    let m = textureSample(mask_atlas, atlas_sampler, mask_uv, mask_layer).r;
    let c = textureSample(color_atlas, atlas_sampler, color_uv, color_layer);
    let cc = clip_coverage(device_pos(in.p));
    return finish_color(select(in.color * m, c * in.color.a, in.layer_kind.y == 1u) * cc);
}
