use kurbo::Affine;
use pebbles_render::paint::{Glyph, Painter, Scene};
use peniko::{Brush, Color, Fill, Mix};

use super::{snap_baseline, subpixel_bin};
use crate::prepare::DrawCmd;
use crate::prepare::test_util::prepare_scene;
use crate::stats::Unsupported;

fn font() -> peniko::FontData {
    crate::test_support::bundled_font()
}

fn run(scene: &mut Scene, size: f32, coords: &[i16], t: Affine, glyphs: Vec<Glyph>) {
    Painter::new(scene).draw_glyphs(
        &font(),
        size,
        coords,
        &Brush::Solid(Color::BLACK),
        t,
        None,
        Fill::NonZero,
        glyphs.into_iter(),
    );
}

fn g(id: u32, x: f32) -> Glyph {
    Glyph { id, x, y: 20.0 }
}

#[test]
fn nan_font_size_skips_run() {
    let mut s = Scene::new();
    run(&mut s, f32::NAN, &[], Affine::IDENTITY, vec![g(40, 0.0)]);
    let (plan, stats) = prepare_scene(&s, 64, 64);
    assert!(plan.glyphs.is_empty());
    assert_eq!(stats.non_finite_skipped, 1);
    assert!(stats.unsupported[Unsupported::NonFinite as usize] > 0);
    assert_eq!(stats.unsupported_kinds(), [Unsupported::NonFinite], "S50: exact key set");
}

#[test]
fn zero_font_size_skips_run() {
    let mut s = Scene::new();
    run(&mut s, 0.0, &[], Affine::IDENTITY, vec![g(40, 0.0)]);
    let (plan, stats) = prepare_scene(&s, 64, 64);
    assert!(plan.glyphs.is_empty() && plan.cmds.is_empty());
    assert_eq!(stats.non_finite_skipped, 0, "silent");
}

#[test]
fn tiny_ppem_skips_run() {
    let mut s = Scene::new();
    run(&mut s, 0.4, &[], Affine::IDENTITY, vec![g(40, 0.0)]);
    let (plan, _) = prepare_scene(&s, 64, 64);
    assert!(plan.glyphs.is_empty());
}

#[test]
fn glyph_id_above_u16_skipped() {
    let mut s = Scene::new();
    run(&mut s, 16.0, &[], Affine::IDENTITY, vec![g(70_000, 0.0), g(40, 10.0)]);
    let (plan, stats) = prepare_scene(&s, 64, 64);
    assert_eq!(plan.glyphs.len(), 1);
    assert_eq!(stats.unsupported[Unsupported::GlyphIdRange as usize], 1);
    assert_eq!(stats.unsupported_kinds(), [Unsupported::GlyphIdRange], "S50: exact key set");
}

#[test]
fn nan_pen_skips_glyph_only() {
    let mut s = Scene::new();
    run(&mut s, 16.0, &[], Affine::IDENTITY, vec![g(40, f32::NAN), g(41, 10.0), g(42, 20.0)]);
    let (plan, stats) = prepare_scene(&s, 64, 64);
    assert_eq!(plan.glyphs.len(), 2, "the other glyphs still draw");
    assert_eq!(stats.non_finite_skipped, 1);
}

#[test]
fn out_of_range_coords_skip_run() {
    let mut s = Scene::new();
    run(&mut s, 16.0, &[20_000], Affine::IDENTITY, vec![g(40, 0.0)]);
    let (plan, stats) = prepare_scene(&s, 64, 64);
    assert!(plan.glyphs.is_empty());
    assert_eq!(stats.non_finite_skipped, 1);
}

#[test]
fn negative_baseline_rounds_down() {
    assert_eq!(snap_baseline(-0.7), Some(-1));
}

#[test]
fn bin_carries_to_next_pixel() {
    assert_eq!(subpixel_bin(0.9), Some((1, 0)));
    assert_eq!(subpixel_bin(-0.3), Some((-1, 3)));
}

// ---- R25 ----

#[test]
fn offscreen_glyphs_are_not_rasterized() {
    // 10 000 glyphs 20 px apart along x; only those near the 200-px-wide target are kept.
    let mut s = Scene::new();
    run(&mut s, 16.0, &[], Affine::IDENTITY, (0..10_000).map(|i| g(36 + i % 26, i as f32 * 20.0)).collect());
    let (plan, _) = prepare_scene(&s, 200, 100);
    assert!(plan.glyphs.len() <= 100 + 26, "{} requests reach the atlas", plan.glyphs.len());
    assert!(plan.stats.culled_glyphs >= 9_800, "culled {}", plan.stats.culled_glyphs);
}

#[test]
fn edge_glyph_is_kept() {
    // Pen 1 px left of the target: its ink still overlaps the target.
    let mut s = Scene::new();
    run(&mut s, 16.0, &[], Affine::IDENTITY, vec![g(40, -1.0)]);
    let (plan, _) = prepare_scene(&s, 64, 64);
    assert_eq!(plan.glyphs.len(), 1);
    assert_eq!(plan.stats.culled_glyphs, 0);
}

#[test]
fn culling_keeps_order() {
    let mut s = Scene::new();
    {
        let mut p = Painter::new(&mut s);
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &kurbo::Rect::new(0.0, 0.0, 4.0, 4.0));
        p.push_layer(
            Fill::NonZero,
            Mix::Normal,
            1.0,
            Affine::IDENTITY,
            &kurbo::Rect::new(0.0, 0.0, 30.0, 64.0),
        );
    }
    run(&mut s, 16.0, &[], Affine::IDENTITY, vec![g(40, 0.0), g(41, 200.0)]);
    {
        let mut p = Painter::new(&mut s);
        p.pop_layer();
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &kurbo::Rect::new(60.0, 0.0, 64.0, 4.0));
    }
    let (plan, _) = prepare_scene(&s, 64, 64);
    let kinds: Vec<bool> = plan.cmds.iter().map(|c| matches!(c, DrawCmd::Glyphs { .. })).collect();
    assert_eq!(kinds, [false, true, false]);
    assert_eq!(plan.glyphs.len(), 1, "the glyph outside the clip is culled");
}

/// R27 AC1: 02 §7.8 — hinted at 1× / 16 px; never at 2× or 64 px.
#[test]
fn hint_policy_matrix() {
    use super::hint_policy;
    assert!(hint_policy(1.0, 16.0));
    assert!(hint_policy(1.75, 48.0));
    assert!(!hint_policy(2.0, 16.0));
    assert!(!hint_policy(1.0, 64.0));
}

/// R27: atlas-route keys carry the policy's hint flag; the 2× root transform turns it off.
#[test]
fn atlas_keys_follow_hint_policy() {
    let key_hint = |scale: f64| {
        let mut s = Scene::new();
        run(&mut s, 16.0, &[], Affine::scale(scale), vec![g(40, 0.0)]);
        let input = crate::prepare::PrepareInput {
            target_size: (128, 128),
            root_transform: Affine::IDENTITY,
            vertex_limit: u32::MAX,
            scale_factor: scale,
            glyph_hint: None,
            lcd: None,
        };
        let (plan, _) = crate::prepare::test_util::prepare_with(&s, &input);
        match plan.glyphs[0].key {
            crate::execute::resources::atlas_alloc::CoverageKey::Glyph(k) => k.hint,
            _ => panic!("glyph expected"),
        }
    };
    assert!(key_hint(1.0));
    assert!(!key_hint(2.0));
}

/// P12: moving a window to a monitor with another scale re-rasterizes its text at the new device
/// size — the atlas key carries the device-pixel size, so a 1× glyph is never stretched to 1.5×.
#[test]
fn scale_change_rekeys_glyphs_at_device_size() {
    let size_q = |scale: f64| {
        let mut s = Scene::new();
        // The shell paints in logical px under the root transform `scale(scale_factor)`.
        run(&mut s, 16.0, &[], Affine::scale(scale), vec![g(40, 0.0)]);
        let input = crate::prepare::PrepareInput {
            target_size: (256, 256),
            root_transform: Affine::IDENTITY,
            vertex_limit: u32::MAX,
            scale_factor: scale,
            glyph_hint: None,
            lcd: None,
        };
        let (plan, _) = crate::prepare::test_util::prepare_with(&s, &input);
        match plan.glyphs[0].key {
            crate::execute::resources::atlas_alloc::CoverageKey::Glyph(k) => k.size_q,
            _ => panic!("glyph expected"),
        }
    };
    assert_eq!(size_q(1.0), 16 * 64);
    assert_eq!(size_q(1.5), 24 * 64);
    assert_eq!(size_q(1.25), 20 * 64);
    assert_eq!(size_q(2.0), 32 * 64);
}

/// Gravel "blurry text": a fractional scroll offset (translate by non-integer px) never puts a
/// glyph quad at a fractional device position — the baseline snaps to a whole pixel and the pen x
/// to a whole pixel plus one of 4 quantised subpixel variants, so atlas texels map 1:1 to pixels.
#[test]
fn fractional_scroll_keeps_glyph_origins_on_pixels() {
    let mut variants = std::collections::BTreeSet::new();
    for k in 0..40 {
        let dy = 3.0 + f64::from(k) * 0.137;
        let dx = f64::from(k) * 0.061;
        let mut s = Scene::new();
        let glyphs = (0..8).map(|i| Glyph { id: 40 + i, x: 7.3 * i as f32 + 0.11, y: 20.0 }).collect();
        run(&mut s, 13.0, &[], Affine::translate((dx, -dy)), glyphs);
        let (plan, _) = prepare_scene(&s, 128, 64);
        assert_eq!(plan.glyphs.len(), 8);
        for gr in &plan.glyphs {
            assert_eq!(gr.origin[0].fract(), 0.0, "x origin {}", gr.origin[0]);
            assert_eq!(gr.origin[1].fract(), 0.0, "y origin {}", gr.origin[1]);
            let crate::execute::resources::atlas_alloc::CoverageKey::Glyph(key) = gr.key else {
                panic!("glyph expected")
            };
            assert!(key.subpx_x < 4);
            assert!(key.hint, "1× / 13 px is hinted");
            variants.insert(key.subpx_x);
        }
    }
    assert_eq!(variants.len(), 4, "exactly 4 horizontal variants: {variants:?}");
}

/// LCD text (`crate::text_aa`): atlas-route runs at the root get LCD keys; runs inside an
/// offscreen layer (transparent intermediate) and rotated runs (outline route) stay grayscale.
#[test]
fn lcd_keys_only_on_opaque_axis_aligned_runs() {
    use crate::execute::resources::atlas_alloc::CoverageKey;
    let mut s = Scene::new();
    run(&mut s, 13.0, &[], Affine::IDENTITY, vec![g(40, 0.0)]);
    run(&mut s, 13.0, &[], Affine::scale(1.5), vec![g(41, 10.0)]);
    Painter::new(&mut s).push_layer(
        Fill::NonZero,
        Mix::Multiply,
        0.5,
        Affine::IDENTITY,
        &kurbo::Rect::new(0.0, 0.0, 128.0, 128.0),
    );
    run(&mut s, 13.0, &[], Affine::IDENTITY, vec![g(42, 20.0)]);
    Painter::new(&mut s).pop_layer();
    run(&mut s, 13.0, &[], Affine::rotate(0.3), vec![g(43, 30.0)]);
    let lcd_codes = |lcd| {
        let input = crate::prepare::PrepareInput {
            target_size: (128, 128),
            root_transform: Affine::IDENTITY,
            vertex_limit: u32::MAX,
            scale_factor: 1.0,
            glyph_hint: None,
            lcd,
        };
        let (plan, _) = crate::prepare::test_util::prepare_with(&s, &input);
        plan.glyphs
            .iter()
            .filter_map(|r| match r.key {
                CoverageKey::Glyph(k) => Some((k.glyph_id, k.lcd)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let on = lcd_codes(Some(crate::text_aa::LcdOrder::Rgb));
    assert!(on.contains(&(40, 1)) && on.contains(&(41, 1)), "root runs (uniform scale too): {on:?}");
    assert!(on.contains(&(42, 0)), "offscreen layer stays grayscale: {on:?}");
    assert!(on.iter().all(|&(id, lcd)| id != 43 || lcd == 0), "rotated run: {on:?}");
    let bgr = lcd_codes(Some(crate::text_aa::LcdOrder::Bgr));
    assert!(bgr.contains(&(40, 2)));
    assert!(lcd_codes(None).iter().all(|&(_, lcd)| lcd == 0), "grayscale frame");
}
