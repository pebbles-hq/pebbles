// Subpixel (LCD) glyphs (`crate::text_aa`): appended to glyph.wgsl in a module that starts with
// `enable dual_source_blending;`. Blend {One, OneMinusSrc1}: dst' = color + dst · (1 − blend)
// per channel, so each subpixel gets its own coverage (Skia / GPUI / WebRender practice).
// Kind 5 (`KIND_TEXT_LCD`) reads RGB coverage from the color pages and applies the text
// contrast / gamma curve per channel; every other kind reduces to premultiplied source-over
// (blend = alpha). CPU mirror: `text_aa::lcd_blend_reference`.

struct LcdOut {
    @location(0) @blend_src(0) color: vec4<f32>,
    @location(0) @blend_src(1) blend: vec4<f32>,
}

@fragment
fn fs_lcd(in: QuadOut) -> LcdOut {
    let is_lcd = in.layer_kind.y == 5u;
    // Kind 5 samples the color pages; glyph_premul's own sample of them stays in uniform flow.
    var g = in;
    g.layer_kind = select(in.layer_kind, vec2<u32>(in.layer_kind.x, 1u), is_lcd);
    let gray = glyph_premul(g);
    let straight = select(vec3<f32>(0.0), in.color.rgb / in.color.a, in.color.a > 0.0);
    let cov = atlas_bilinear(color_atlas, in.atlas_px, select(0u, in.layer_kind.x, is_lcd)).rgb;
    let cc = clip_coverage(device_pos(in.p)) * clip_mask_coverage(device_pos(in.p));
    let m = vec3<f32>(
        text_coverage(cov.r, straight),
        text_coverage(cov.g, straight),
        text_coverage(cov.b, straight),
    ) * cc;
    let m_max = max(m.r, max(m.g, m.b));
    var o: LcdOut;
    o.color = finish_color(select(gray, vec4<f32>(in.color.rgb * m, in.color.a * m_max), is_lcd));
    o.blend = select(vec4<f32>(gray.a), in.color.a * vec4<f32>(m, m_max), is_lcd);
    return o;
}
