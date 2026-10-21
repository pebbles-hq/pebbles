//! FramePlan snapshot tests (06 §10.3): deterministic `debug_dump` goldens.

use kurbo::Affine;
use kurbo::{Cap, Join};
use peniko::{Brush, Color, Fill, Mix};

use super::{DrawCmd, FramePlan, PrepareInput, PrepareScratch, prepare};
use pebbles_render::paint::{Glyph, Painter, Scene, Stroke};

fn plan_of(scene: &Scene, size: (u32, u32)) -> FramePlan {
    let mut scratch = PrepareScratch::default();
    let input = PrepareInput {
        target_size: size,
        root_transform: Affine::IDENTITY,
        vertex_limit: u32::MAX,
        scale_factor: 1.0,
        glyph_hint: None,
    };
    prepare(
        scene,
        &input,
        &crate::test_support::webgl2_limits(),
        &mut scratch,
        &mut crate::stats::FrameStats::default(),
    );
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
    // Shadow, then the card fill (R10) and its border (R11) as Tier A shapes, merged.
    assert_eq!(plan.cmds.len(), 2);
    assert!(matches!(plan.cmds[0], DrawCmd::Shadows { .. }));
    assert!(matches!(plan.cmds[1], DrawCmd::Shapes { .. }));
    check_snapshot("card_with_shadow", &plan.debug_dump());
}

/// Five glyphs at fixed ids and advances — no shaping, so the dump is OS-independent.
fn glyph_run(p: &mut Painter<'_>, x: f32, y: f32) {
    let font = crate::test_support::bundled_font();
    let glyphs = [36u32, 72, 79, 79, 82].into_iter().enumerate().map(move |(i, id)| Glyph {
        id,
        x: x + 10.0 * i as f32,
        y,
    });
    p.draw_glyphs(
        &font,
        14.0,
        &[],
        &Brush::Solid(Color::BLACK),
        Affine::IDENTITY,
        None,
        Fill::NonZero,
        glyphs,
    );
}

/// A 16×16 procedural checker (premultiplied, opaque).
fn checker() -> peniko::ImageBrush {
    let mut data = Vec::with_capacity(16 * 16 * 4);
    for y in 0..16u32 {
        for x in 0..16u32 {
            let v = if (x / 4 + y / 4) % 2 == 0 { 255 } else { 64 };
            data.extend_from_slice(&[v, v, v, 255]);
        }
    }
    peniko::ImageBrush::new(peniko::ImageData {
        data: peniko::Blob::from(data),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::AlphaPremultiplied,
        width: 16,
        height: 16,
    })
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> kurbo::Rect {
    kurbo::Rect::new(x0, y0, x1, y1)
}

fn clipped_list() -> FramePlan {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &rect(10.0, 10.0, 190.0, 110.0));
    for i in 0..5 {
        let y = 10.0 + 24.0 * i as f64;
        let shade = if i % 2 == 0 { 240 } else { 225 };
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(shade, shade, shade, 255),
            None,
            &rect(10.0, y, 190.0, y + 24.0),
        );
        glyph_run(&mut p, 16.0, 27.0 + 24.0 * i as f32);
    }
    p.pop_layer();
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0, 120, 215, 255),
        None,
        &rect(10.0, 150.0, 190.0, 190.0),
    );
    plan_of(&scene, (200, 200))
}

fn dialog_over_page() -> FramePlan {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &rect(0.0, 0.0, 200.0, 200.0));
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0, 120, 215, 255),
        None,
        &rect(0.0, 0.0, 200.0, 32.0),
    );
    glyph_run(&mut p, 10.0, 50.0);
    p.draw_image(&checker(), Affine::translate((10.0, 60.0)));
    // The scrim covers everything painted so far; the dialog paints over it.
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0, 0, 0, 102),
        None,
        &rect(0.0, 0.0, 200.0, 200.0),
    );
    let card = kurbo::RoundedRect::new(30.0, 60.0, 170.0, 140.0, 8.0);
    p.draw_blurred_rounded_rect(Affine::IDENTITY, card.rect(), Color::from_rgba8(0, 0, 0, 102), 8.0, 6.0);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::WHITE, None, &card);
    glyph_run(&mut p, 40.0, 90.0);
    plan_of(&scene, (200, 200))
}

fn icon_paths() -> FramePlan {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    let stroke = Stroke::new(1.5).with_caps(Cap::Round).with_join(Join::Round);
    for i in 0..6 {
        let ox = 10.0 + 30.0 * i as f64;
        let mut path = kurbo::BezPath::new();
        path.move_to((ox, 20.0));
        path.line_to((ox + 20.0, 20.0));
        path.quad_to((ox + 24.0, 32.0), (ox + 10.0, 44.0));
        path.curve_to((ox + 4.0, 38.0), (ox - 2.0, 30.0), (ox, 20.0));
        path.close_path();
        p.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgba8(40, 40, 40, 255), None, &path);
        p.stroke(&stroke, Affine::translate((0.0, 60.0)), Color::from_rgba8(200, 40, 40, 255), None, &path);
    }
    plan_of(&scene, (200, 200))
}

fn image_text_image() -> FramePlan {
    let image = checker();
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.draw_image(&image, Affine::translate((10.0, 10.0)));
    glyph_run(&mut p, 10.0, 50.0);
    p.draw_image(&image, Affine::translate((10.0, 70.0)));
    plan_of(&scene, (200, 200))
}

#[test]
fn snapshot_clipped_list() {
    check_snapshot("clipped_list", &clipped_list().debug_dump());
}

#[test]
fn snapshot_dialog_over_page() {
    let plan = dialog_over_page();
    // Painter's order is law (DR-04): the scrim's fill lands after the page and before the dialog.
    let scrim = plan
        .cmds
        .iter()
        .position(|c| matches!(c, DrawCmd::Images { .. }))
        .map(|i| i + 1)
        .expect("page image");
    // Full-rect fills take Tier A (`Shapes`) or the tessellated `Solid` path.
    assert!(
        matches!(plan.cmds[scrim], DrawCmd::Solid { .. } | DrawCmd::Shapes { .. }),
        "{}",
        plan.debug_dump()
    );
    assert!(matches!(plan.cmds[scrim + 1], DrawCmd::Shadows { .. }), "{}", plan.debug_dump());
    check_snapshot("dialog_over_page", &plan.debug_dump());
}

#[test]
fn snapshot_icon_paths() {
    check_snapshot("icon_paths", &icon_paths().debug_dump());
}

#[test]
fn snapshot_image_text_image() {
    let plan = image_text_image();
    let images = plan.cmds.iter().filter(|c| matches!(c, DrawCmd::Images { .. })).count();
    assert_eq!(images, 2, "{}", plan.debug_dump());
    check_snapshot("image_text_image", &plan.debug_dump());
}

#[test]
fn snapshot_dump_is_deterministic() {
    assert_eq!(dialog_over_page().debug_dump(), dialog_over_page().debug_dump());
}
