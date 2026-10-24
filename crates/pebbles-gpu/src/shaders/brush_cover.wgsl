// The coverage variants shared by every brush (09 §6 brush × tier rule): Shape (Tier A
// instances), Mask (Tier B path-mask quads read from the mask atlas at group 0 binding 3) and
// Mesh (Tier C lyon meshes inside the MSAA run). The including shader provides
// `brush_paint(p) -> premultiplied color` and `brush_post(c, p)` (gradients dither).
fn brush_out(p: vec2<f32>, coverage: f32) -> vec4<f32> {
    return brush_post(brush_paint(p) * (coverage * clip_coverage(p) * clip_mask_coverage(p)), p);
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
    return finish_color(brush_out(device_pos(in.pos), in.alpha));
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
    return finish_color(brush_out(device_pos(in.clip), coverage * in.alpha));
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
    // No `textureNumLayers` clamp: naga's HLSL backend returns the MIP count for arrayed
    // textures (always 1 here), which pinned every slot to layer 0 on DX12. Slots are valid layers.
    let layer = i32(in.layer);
    let m = textureLoad(clip_atlas, texel, layer, 0).r;
    return finish_color(brush_out(device_pos(in.p), m * in.alpha));
}
