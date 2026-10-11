//! Prepare: walk the shared display list into an ordered [`FramePlan`] on the CPU (no device).

use kurbo::Affine;

use pebbles_render::paint::Scene;
use pebbles_render::paint::display_list::Op;

mod clip;
pub(crate) mod color;
mod glyphs;
mod images;
mod plan;
#[cfg(test)]
mod property_tests;
mod shadows;
#[cfg(test)]
mod snapshot_tests;
mod stroke;
mod tessellate;

pub(crate) use images::{ImageUpload, image_uploads};
pub(crate) use plan::{DrawCmd, FramePlan, GlyphRun, ImageQuad, ScissorRect};

/// Path-flattening tolerance for recorded shapes (logical px) — a local copy, because
/// `display_list::TOL` is `pub(crate)` to pebbles-render.
pub(crate) const TOL: f64 = 0.1;

/// Per-frame prepare inputs.
pub(crate) struct PrepareInput {
    pub target_size: (u32, u32),
    /// Logical → device transform for the whole scene (identity during the transition).
    pub root_transform: Affine,
}

/// Reusable per-Renderer prepare state (zero steady-state allocation).
#[derive(Default)]
pub(crate) struct PrepareScratch {
    pub plan: FramePlan,
    pub clips: clip::ClipStack,
    pub stroke: stroke::StrokeScratch,
}

/// Walk `scene` in painter's order into `scratch.plan` and return it.
pub(crate) fn prepare<'s>(
    scene: &Scene,
    input: &PrepareInput,
    scratch: &'s mut PrepareScratch,
) -> &'s FramePlan {
    scratch.plan.clear();
    scratch.plan.target_size = input.target_size;
    scratch.clips.reset(input.target_size);
    let mut visit = 0u32;
    emit(
        scene.ops(),
        input.root_transform,
        &mut scratch.plan,
        &mut scratch.clips,
        &mut scratch.stroke,
        &mut visit,
    );
    &scratch.plan
}

#[allow(clippy::too_many_arguments)]
fn emit(
    ops: &[Op],
    outer: Affine,
    plan: &mut FramePlan,
    clips: &mut clip::ClipStack,
    stroke: &mut stroke::StrokeScratch,
    visit: &mut u32,
) {
    for op in ops {
        *visit += 1;
        let idx = *visit;
        plan.stats.ops += 1;
        match op {
            Op::Fill { style, transform, brush: peniko::Brush::Solid(color), shape, .. } => {
                if clips.is_culled() {
                    plan.stats.culled += 1;
                    continue;
                }
                let path = shape.to_path(TOL);
                let indices = tessellate::fill(plan, &path, outer * *transform, *style, *color);
                plan.stats.tess_calls += 1;
                push_cmd(plan, DrawCmd::Solid { indices, clip: clips.current() }, idx);
            }
            Op::Stroke { style, transform, brush: peniko::Brush::Solid(color), shape, .. } => {
                if clips.is_culled() {
                    plan.stats.culled += 1;
                    continue;
                }
                let path = shape.to_path(TOL);
                let outcome = stroke::expand(&path, style, outer * *transform, stroke);
                if outcome.drew {
                    let premul = color::premul_scaled(*color, outcome.alpha);
                    let indices = stroke::fill_outline(plan, stroke, premul);
                    plan.stats.tess_calls += 1;
                    push_cmd(plan, DrawCmd::Solid { indices, clip: clips.current() }, idx);
                }
            }
            Op::PushLayer { blend, alpha, transform, clip, .. } => {
                let scissor = if *blend == peniko::BlendMode::default() && *alpha >= 1.0 {
                    let clip_path = clip.to_path(TOL);
                    clip::scissor_rect(&clip_path, outer * *transform)
                } else {
                    None
                };
                clips.push(scissor);
            }
            Op::PopLayer => clips.pop(),
            Op::Glyphs {
                font, font_size, font_weight, brush, transform, glyph_transform, glyphs, ..
            } => {
                if clips.is_culled() {
                    plan.stats.culled += 1;
                    continue;
                }
                if let Some(range) = glyphs::glyph_run(
                    plan,
                    font,
                    *font_size,
                    *font_weight,
                    brush,
                    outer * *transform,
                    *glyph_transform,
                    glyphs,
                ) {
                    push_cmd(plan, DrawCmd::Glyphs { glyphs: range, clip: clips.current() }, idx);
                }
            }
            Op::Fragment { transform, ops, .. } => {
                emit(ops, outer * transform.unwrap_or(Affine::IDENTITY), plan, clips, stroke, visit);
            }
            Op::Image { image, transform, .. } if image.image.width != 0 && image.image.height != 0 => {
                if clips.is_culled() {
                    plan.stats.culled += 1;
                    continue;
                }
                let range = images::image(plan, image, outer * *transform, plan.images.len() as u32);
                push_cmd(plan, DrawCmd::Images { images: range, clip: clips.current() }, idx);
            }
            Op::BlurredRect { transform, rect, brush, radius, std_dev, .. } => {
                if clips.is_culled() {
                    plan.stats.culled += 1;
                    continue;
                }
                let range = shadows::shadow(plan, rect, outer * *transform, *radius, *std_dev, *brush);
                push_cmd(plan, DrawCmd::Shadows { instances: range, clip: clips.current() }, idx);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
fn push_cmd(plan: &mut FramePlan, cmd: DrawCmd, visit: u32) {
    if plan.push_draw(cmd) {
        plan.cmd_ops.push(visit);
    }
}

#[cfg(not(test))]
fn push_cmd(plan: &mut FramePlan, cmd: DrawCmd, _visit: u32) {
    plan.push_draw(cmd);
}

#[cfg(test)]
mod tests {
    use super::*;
    use pebbles_render::paint::{Glyph, Painter, Stroke};
    use peniko::{Brush, Color, Fill};

    fn plan_of(scene: &Scene, size: (u32, u32)) -> FramePlan {
        let mut scratch = PrepareScratch::default();
        let input = PrepareInput { target_size: size, root_transform: Affine::IDENTITY };
        prepare(scene, &input, &mut scratch);
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
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::WHITE,
            None,
            &kurbo::Rect::new(20.0, 20.0, 180.0, 120.0),
        );

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
            400,
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
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::WHITE,
            None,
            &kurbo::Rect::new(40.0, 40.0, 160.0, 120.0),
        );
        p.draw_glyphs(
            &font,
            32.0,
            400,
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
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::WHITE,
            None,
            &kurbo::Rect::new(20.0, 20.0, 30.0, 30.0),
        );

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
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::WHITE,
            None,
            &kurbo::Rect::new(20.0, 20.0, 30.0, 30.0),
        );
        let font = crate::test_support::bundled_font();
        p.draw_glyphs(
            &font,
            32.0,
            400,
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
            Color::WHITE,
            None,
            &kurbo::Rect::new(40.0, 40.0, 50.0, 50.0),
        );

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
    fn rotated_clip_is_not_scissored() {
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
            DrawCmd::Solid { clip, .. } => assert_eq!(clip.scissor, full((200, 200))),
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
}
