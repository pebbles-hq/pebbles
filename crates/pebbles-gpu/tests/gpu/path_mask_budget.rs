//! M28: Tier B path masks stay under their own coverage budget and never evict glyphs.

use super::*;

/// A distinct 20 px star per `i`.
fn icon(i: u32) -> kurbo::BezPath {
    let mut p = kurbo::BezPath::new();
    let r_in = 3.0 + f64::from(i) * 0.003;
    for k in 0..10 {
        let r = if k % 2 == 0 { 10.0 } else { r_in };
        let a = std::f64::consts::PI * f64::from(k) / 5.0;
        let pt = (10.0 + r * a.sin(), 10.0 - r * a.cos());
        if k == 0 {
            p.move_to(pt);
        } else {
            p.line_to(pt);
        }
    }
    p.close_path();
    p
}

#[test]
#[ignore = "gpu"]
fn path_mask_budget_holds_over_2000_icons() {
    // ~15 icon masks fit; each frame draws 20 new ones, so 5 per frame tessellate instead.
    let budget = 15 * 26 * 26;
    let Some((device, _queue, mut renderer)) =
        setup_with_budgets("path_mask_budget_holds_over_2000_icons", |b| b.path_mask_bytes = budget)
    else {
        return;
    };
    let bytes = pebbles_render::fonts::builtin_fonts()[0].1;
    let font = peniko::FontData::new(peniko::Blob::from(bytes.to_vec()), 0);
    let mut targets = FrameTargets::new();
    for frame in 0..100u32 {
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        let glyphs: Vec<Glyph> = (0..20).map(|k| Glyph { id: 36 + k, x: k as f32 * 10.0, y: 0.0 }).collect();
        p.draw_glyphs(
            &font,
            14.0,
            &[],
            &Brush::Solid(Color::BLACK),
            Affine::translate((4.0, 300.0)),
            None,
            Fill::NonZero,
            glyphs.into_iter(),
        );
        for k in 0..20u32 {
            let xf = Affine::translate((f64::from(k % 10) * 30.0, f64::from(k / 10) * 30.0));
            p.fill(Fill::NonZero, xf, Color::from_rgb8(16, 185, 129), None, &icon(frame * 20 + k));
        }
        let texture = target_texture(&device, 320, 320);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let target = RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, &mut targets);
        let stats = renderer.render(target, &s, &FrameParams::new(Color::WHITE)).expect("render");
        #[cfg(debug_assertions)]
        {
            let c = renderer.census();
            assert!(c.path_mask_bytes <= budget, "frame {frame}: {} > {budget}", c.path_mask_bytes);
        }
        if frame > 0 {
            assert_eq!(stats.glyphs_rasterized, 0, "frame {frame}: a glyph used every frame was evicted");
        }
        assert!(stats.tier_b_draws >= 1 && stats.tier_c_draws >= 1, "frame {frame}: {stats:?}");
    }
}
