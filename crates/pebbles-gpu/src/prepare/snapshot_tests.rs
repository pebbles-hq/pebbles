//! FramePlan snapshot tests (06 §10.3): deterministic `debug_dump` goldens.

use kurbo::Affine;
use peniko::{Color, Fill};

use super::{DrawCmd, FramePlan, PrepareInput, PrepareScratch, prepare};
use pebbles_render::paint::{Painter, Scene, Stroke};

fn plan_of(scene: &Scene, size: (u32, u32)) -> FramePlan {
    let mut scratch = PrepareScratch::default();
    let input = PrepareInput { target_size: size, root_transform: Affine::IDENTITY, vertex_limit: u32::MAX };
    prepare(scene, &input, &mut scratch, &mut crate::stats::FrameStats::default());
    scratch.plan
}

/// The shadowed-card scene: a blurred rounded rect under a white fill with a gray stroke.
fn card_with_shadow() -> FramePlan {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.draw_blurred_rounded_rect(
        Affine::IDENTITY,
        kurbo::Rect::new(20.0, 20.0, 180.0, 120.0),
        Color::from_rgba8(0, 0, 0, 153),
        12.0,
        8.0,
    );
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &kurbo::Rect::new(20.0, 20.0, 180.0, 120.0));
    p.stroke(
        &Stroke::new(1.0),
        Affine::IDENTITY,
        Color::from_rgba8(128, 128, 128, 255),
        None,
        &kurbo::Rect::new(20.0, 20.0, 180.0, 120.0),
    );
    plan_of(&scene, (200, 160))
}

fn check_snapshot(name: &str, actual: &str) {
    let path = format!("{}/tests/snapshots/{name}.plan.txt", env!("CARGO_MANIFEST_DIR"));
    if std::env::var("PEBBLES_UPDATE_GOLDENS").is_ok() {
        std::fs::write(&path, actual).expect("write golden");
        return;
    }
    let expected =
        std::fs::read_to_string(&path).expect("read golden (set PEBBLES_UPDATE_GOLDENS=1 to write)");
    // Git may check the golden out with CRLF on Windows — normalize before comparing.
    let expected = expected.replace("\r\n", "\n");
    assert_eq!(
        actual.trim(),
        expected.trim(),
        "snapshot {name} differs; set PEBBLES_UPDATE_GOLDENS=1 to update"
    );
}

#[test]
fn card_with_shadow_snapshot() {
    let plan = card_with_shadow();
    assert_eq!(plan.cmds.len(), 2);
    assert!(matches!(plan.cmds[0], DrawCmd::Shadows { .. }));
    assert!(matches!(plan.cmds[1], DrawCmd::Solid { .. }));
    check_snapshot("card_with_shadow", &plan.debug_dump());
}
