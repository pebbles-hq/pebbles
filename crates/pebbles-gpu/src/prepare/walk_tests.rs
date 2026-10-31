//! Painter's-order, merge, clip and transform tests for the prepare walk (C05, C07, C11).

use super::*;
use pebbles_render::paint::{Glyph, Painter, Stroke};
use peniko::{Brush, Color, Fill};

/// The tessellated tier (Tier A off): these tests use rects as fixtures for walk properties.
fn plan_of(scene: &Scene, size: (u32, u32)) -> FramePlan {
    let mut scratch = PrepareScratch { sdf_disabled: true, ..PrepareScratch::default() };
    let input = PrepareInput {
        target_size: size,
        root_transform: Affine::IDENTITY,
        vertex_limit: u32::MAX,
        scale_factor: 1.0,
        glyph_hint: None,
        lcd: None,
    };
    prepare(scene, &input, &crate::test_support::webgl2_limits(), &mut scratch, &mut FrameStats::default());
    scratch.plan
}

fn full(size: (u32, u32)) -> ScissorRect {
    ScissorRect { x0: 0, y0: 0, x1: size.0, y1: size.1 }
}

fn glyph() -> Glyph {
    Glyph { id: 36, x: 10.0, y: 10.0 }
}

#[test]
fn shadow_draws_before_card_fill() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.draw_blurred_rounded_rect(
        Affine::IDENTITY,
        kurbo::Rect::new(20.0, 20.0, 180.0, 120.0),
        Color::BLACK,
        12.0,
        8.0,
    );
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &kurbo::Rect::new(20.0, 20.0, 180.0, 120.0));

    let plan = plan_of(&scene, (200, 160));
    assert_eq!(plan.cmds.len(), 2);
    assert!(matches!(plan.cmds[0], DrawCmd::Shadows { .. }));
    assert!(matches!(plan.cmds[1], DrawCmd::Solid { .. }));
}

#[test]
fn text_after_scrim_draws_after_it() {
    let font = crate::test_support::bundled_font();
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.draw_glyphs(
        &font,
        32.0,
        &[],
        &Brush::Solid(Color::BLACK),
        Affine::IDENTITY,
        None,
        Fill::NonZero,
        std::iter::once(glyph()),
    );
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0, 0, 0, 128),
        None,
        &kurbo::Rect::new(0.0, 0.0, 200.0, 160.0),
    );
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &kurbo::Rect::new(40.0, 40.0, 160.0, 120.0));
    p.draw_glyphs(
        &font,
        32.0,
        &[],
        &Brush::Solid(Color::BLACK),
        Affine::IDENTITY,
        None,
        Fill::NonZero,
        std::iter::once(Glyph { id: 36, x: 50.0, y: 50.0 }),
    );

    let plan = plan_of(&scene, (200, 160));
    assert_eq!(plan.cmds.len(), 3);
    assert!(matches!(plan.cmds[0], DrawCmd::Glyphs { .. }));
    assert!(matches!(plan.cmds[1], DrawCmd::Solid { .. }));
    assert!(matches!(plan.cmds[2], DrawCmd::Glyphs { .. }));
}

#[test]
fn image_between_fills_keeps_its_slot() {
    let image = peniko::ImageData {
        data: peniko::Blob::from(vec![255u8; 100]),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::AlphaPremultiplied,
        width: 5,
        height: 5,
    };
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0));
    p.draw_image(&peniko::ImageBrush::new(image), Affine::IDENTITY);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &kurbo::Rect::new(20.0, 20.0, 30.0, 30.0));

    let plan = plan_of(&scene, (100, 100));
    assert_eq!(plan.cmds.len(), 3);
    assert!(matches!(plan.cmds[0], DrawCmd::Solid { .. }));
    assert!(matches!(plan.cmds[1], DrawCmd::Images { .. }));
    assert!(matches!(plan.cmds[2], DrawCmd::Solid { .. }));
}

#[test]
fn merge_only_adjacent_same_kind() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0));
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &kurbo::Rect::new(20.0, 20.0, 30.0, 30.0));
    let font = crate::test_support::bundled_font();
    p.draw_glyphs(
        &font,
        32.0,
        &[],
        &Brush::Solid(Color::BLACK),
        Affine::IDENTITY,
        None,
        Fill::NonZero,
        std::iter::once(glyph()),
    );
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &kurbo::Rect::new(40.0, 40.0, 50.0, 50.0));

    let plan = plan_of(&scene, (100, 100));
    assert_eq!(plan.cmds.len(), 3);
    assert!(matches!(plan.cmds[0], DrawCmd::Solid { .. }));
    assert!(matches!(plan.cmds[1], DrawCmd::Glyphs { .. }));
    assert!(matches!(plan.cmds[2], DrawCmd::Solid { .. }));
}

#[test]
fn unbalanced_pop_is_ignored_and_counted() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.pop_layer();
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(255, 0, 0, 255),
        None,
        &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
    );

    let plan = plan_of(&scene, (100, 100));
    assert_eq!(plan.cmds.len(), 1);
    assert!(matches!(plan.cmds[0], DrawCmd::Solid { .. }));
    assert_eq!(plan.stats.ops, 2); // the unmatched pop + the fill
}

#[test]
fn fragment_transform_composes() {
    let mut inner = Scene::new();
    Painter::new(&mut inner).fill(
        Fill::NonZero,
        Affine::translate((10.0, 20.0)),
        Color::from_rgba8(255, 0, 0, 255),
        None,
        &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
    );
    let mut scene = Scene::new();
    Painter::new(&mut scene).append(&inner, Some(Affine::translate((5.0, 7.0))));

    let plan = plan_of(&scene, (100, 100));
    assert_eq!(plan.cmds.len(), 1);
    assert!(matches!(plan.cmds[0], DrawCmd::Solid { .. }));
    let min_x = plan.solid_vertices.iter().map(|v| v.pos[0]).fold(f32::MAX, f32::min);
    let min_y = plan.solid_vertices.iter().map(|v| v.pos[1]).fold(f32::MAX, f32::min);
    assert!((min_x - 15.0).abs() < 0.5, "composed x should be ~15, got {min_x}");
    assert!((min_y - 27.0).abs() < 0.5, "composed y should be ~27, got {min_y}");
}

// — rewritten C05 tests (06 §12.2 row B45) —

#[test]
fn solid_fill_tessellates_to_a_single_unclipped_batch() {
    let mut scene = Scene::new();
    Painter::new(&mut scene).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(255, 0, 0, 255),
        None,
        &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
    );

    let plan = plan_of(&scene, (100, 100));
    assert_eq!(plan.cmds.len(), 1);
    match &plan.cmds[0] {
        DrawCmd::Solid { clip, .. } => assert_eq!(clip.scissor, full((100, 100))),
        _ => panic!("expected a Solid command"),
    }
}

#[test]
fn rect_clip_partitions_geometry_into_scissored_batches() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.push_layer(
        Fill::NonZero,
        peniko::BlendMode::default(),
        1.0,
        Affine::IDENTITY,
        &kurbo::Rect::new(0.0, 0.0, 100.0, 50.0),
    );
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0, 255, 0, 255),
        None,
        &kurbo::Rect::new(0.0, 0.0, 50.0, 25.0),
    );
    p.pop_layer();
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0, 0, 255, 255),
        None,
        &kurbo::Rect::new(0.0, 0.0, 10.0, 10.0),
    );

    let plan = plan_of(&scene, (200, 200));
    assert_eq!(plan.cmds.len(), 2);
    match &plan.cmds[0] {
        DrawCmd::Solid { clip, .. } => {
            assert_eq!(clip.scissor, ScissorRect { x0: 0, y0: 0, x1: 100, y1: 50 })
        }
        _ => panic!("expected a clipped Solid"),
    }
    match &plan.cmds[1] {
        DrawCmd::Solid { clip, .. } => assert_eq!(clip.scissor, full((200, 200))),
        _ => panic!("expected an unclipped Solid"),
    }
}

#[test]
fn rotated_clip_is_bounds_clipped() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.push_layer(
        Fill::NonZero,
        peniko::BlendMode::default(),
        1.0,
        Affine::rotate(std::f64::consts::FRAC_PI_4),
        &kurbo::Rect::new(0.0, 0.0, 100.0, 50.0),
    );
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0, 255, 0, 255),
        None,
        &kurbo::Rect::new(0.0, 0.0, 50.0, 25.0),
    );
    p.pop_layer();

    let plan = plan_of(&scene, (200, 200));
    assert_eq!(plan.cmds.len(), 1);
    match &plan.cmds[0] {
        // A rotated rect is not a pure scissor: it clips to its device bounding box.
        DrawCmd::Solid { clip, .. } => {
            assert_eq!(clip.scissor, ScissorRect { x0: 0, y0: 0, x1: 71, y1: 107 })
        }
        _ => panic!("expected a Solid"),
    }
}

#[test]
fn alpha_layer_keeps_its_clip() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.push_layer(
        Fill::NonZero,
        peniko::BlendMode::default(),
        0.5,
        Affine::IDENTITY,
        &kurbo::Rect::new(0.0, 0.0, 50.0, 50.0),
    );
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(255, 0, 0, 255),
        None,
        &kurbo::Rect::new(0.0, 0.0, 100.0, 100.0),
    );
    p.pop_layer();

    let plan = plan_of(&scene, (100, 100));
    assert_eq!(plan.cmds.len(), 1);
    match &plan.cmds[0] {
        DrawCmd::Solid { clip, .. } => {
            assert_eq!(clip.scissor, ScissorRect { x0: 0, y0: 0, x1: 50, y1: 50 })
        }
        _ => panic!("expected a Solid"),
    }
}

#[test]
fn dashed_stroke_tessellates_geometry() {
    let mut scene = Scene::new();
    let style = Stroke::new(2.0).with_dashes(0.0, [6.0, 3.0]);
    Painter::new(&mut scene).stroke(
        &style,
        Affine::IDENTITY,
        Color::from_rgba8(0, 0, 0, 255),
        None,
        &kurbo::Rect::new(0.0, 0.0, 100.0, 40.0),
    );

    let plan = plan_of(&scene, (200, 200));
    assert!(plan.solid_vertices.len() > 4);
}
