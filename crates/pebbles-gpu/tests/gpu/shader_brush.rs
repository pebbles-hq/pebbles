//! Shader brushes (L43, 09 §6.1): app WGSL paints fills on every tier; a failing program draws
//! nothing and is logged once.

use pebbles_render::paint::display_list::PathHint;
use pebbles_render::paint::{ShaderBrush, ShaderProgram};

use super::*;

const UV: &str = "fn pebbles_paint(local: vec2<f32>, device: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(local.x / 64.0, 0.0, 0.0, 1.0);
}";

fn brush(src: &str) -> ShaderBrush {
    let none: [peniko::ImageBrush; 0] = [];
    ShaderBrush::new(ShaderProgram::new(src), [0.0f32; 0], none, Affine::IDENTITY).expect("brush")
}

fn star() -> kurbo::BezPath {
    let mut p = kurbo::BezPath::new();
    for i in 0..10 {
        let r = if i % 2 == 0 { 30.0 } else { 14.0 };
        let a = std::f64::consts::PI * f64::from(i) / 5.0 - std::f64::consts::FRAC_PI_2;
        let pt = (32.0 + r * a.cos(), 32.0 + r * a.sin());
        if i == 0 { p.move_to(pt) } else { p.line_to(pt) }
    }
    p.close_path();
    p
}

fn red_at_center(rgba: &[u8], what: &str) {
    let c = pixel(rgba, 64, 32, 32);
    assert!((i32::from(c[0]) - 128).abs() <= 2 && c[1] == 0 && c[2] == 0 && c[3] == 255, "{what}: {c:?}");
}

#[test]
#[ignore = "gpu"]
fn uv_gradient_program_paints() {
    let Some((device, queue, mut renderer)) = setup("uv_gradient_program_paints") else { return };
    let b = brush(UV);

    // Tier A: an analytic rect.
    let mut scene = Scene::new();
    Painter::new(&mut scene).fill_shader(
        Fill::NonZero,
        Affine::IDENTITY,
        &b,
        &Rect::new(0.0, 0.0, 64.0, 64.0),
    );
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (64, 64), Color::BLACK);
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert_eq!(renderer.last_frame_stats().tier_a_draws, 1);
    red_at_center(&rgba, "tier A");
    let left = pixel(&rgba, 64, 2, 32);
    assert!(left[0] < 16, "local.x grows left → right: {left:?}");

    // Tier B: a star (cached coverage mask).
    let mut scene = Scene::new();
    Painter::new(&mut scene).fill_shader(Fill::NonZero, Affine::IDENTITY, &b, &star());
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (64, 64), Color::BLACK);
    assert_eq!(renderer.last_frame_stats().tier_b_draws, 1);
    red_at_center(&rgba, "tier B");

    // Tier C: a volatile star (MSAA mesh).
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.set_path_hint(PathHint::Volatile);
    p.fill_shader(Fill::NonZero, Affine::IDENTITY, &b, &star());
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (64, 64), Color::BLACK);
    assert_eq!(renderer.last_frame_stats().tier_c_draws, 1);
    red_at_center(&rgba, "tier C");
    assert_eq!(renderer.last_frame_stats().shader_errors, 0);
}

#[test]
#[ignore = "gpu"]
fn shader_brush_uniforms_and_transform() {
    let Some((device, queue, mut renderer)) = setup("shader_brush_uniforms_and_transform") else { return };
    let src = "fn pebbles_paint(local: vec2<f32>, device: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(0.0, u.values[0].y, select(0.0, 1.0, local.x > u.size.x * 0.5), 1.0);
}";
    let none: [peniko::ImageBrush; 0] = [];
    let b = ShaderBrush::new(ShaderProgram::new(src), [0.0f32, 0.5], none, Affine::IDENTITY).expect("brush");
    let mut scene = Scene::new();
    // Under a translation: `local` is the op's local space, so the split follows the rect.
    Painter::new(&mut scene).fill_shader(
        Fill::NonZero,
        Affine::translate((16.0, 0.0)),
        &b,
        &Rect::new(0.0, 0.0, 32.0, 16.0),
    );
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (64, 16), Color::BLACK);
    let (l, r) = (pixel(&rgba, 64, 20, 8), pixel(&rgba, 64, 44, 8));
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert!((i32::from(l[1]) - 128).abs() <= 2 && l[2] == 0, "left half: {l:?}");
    assert!((i32::from(r[1]) - 128).abs() <= 2 && r[2] == 255, "right half: {r:?}");
}

#[test]
#[ignore = "gpu"]
fn failing_program_draws_nothing_and_logs_once() {
    let Some((device, queue, mut renderer)) = setup("failing_program_draws_nothing") else { return };
    let bad = brush("fn pebbles_paint(local: vec2<f32>, device: vec2<f32>) -> vec4<f32> { return oops; }");
    let good = brush(UV);
    let count =
        || pebbles_foundation::log::snapshot().iter().filter(|r| r.msg.starts_with("shader brush")).count();
    let before = count();
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    for _ in 0..2 {
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        p.fill_shader(Fill::NonZero, Affine::IDENTITY, &bad, &Rect::new(0.0, 0.0, 32.0, 64.0));
        p.fill_shader(Fill::NonZero, Affine::IDENTITY, &good, &Rect::new(32.0, 0.0, 64.0, 64.0));
        let rgba = render_scene(&device, &queue, &mut renderer, &scene, (64, 64), Color::WHITE);
        assert!(is_white(pixel(&rgba, 64, 16, 32)), "the failing program draws nothing");
        let c = pixel(&rgba, 64, 48, 32);
        assert!(c[1] == 0 && c[0] > 128, "other draws are not poisoned: {c:?}");
        assert_eq!(renderer.last_frame_stats().shader_errors, 1);
    }
    assert_eq!(count() - before, 1, "logged once");
}
