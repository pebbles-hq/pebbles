//! Group opacity (09 §3.4–§3.7, L12–L14): folded single draws and exact offscreen layers.

use super::*;
use peniko::Mix;

const RED: Color = Color::from_rgba8(255, 0, 0, 255);
const PINK: [u8; 3] = [255, 128, 128];

fn near(c: [u8; 4], want: [u8; 3], tol: i32) -> bool {
    (0..3).all(|i| (i32::from(c[i]) - i32::from(want[i])).abs() <= tol)
}

fn layer(p: &mut Painter<'_>, alpha: f32, xf: Affine, clip: &impl kurbo::Shape) {
    p.push_layer(Fill::NonZero, Mix::Normal, alpha, xf, clip);
}

fn red(p: &mut Painter<'_>, r: Rect) {
    p.fill(Fill::NonZero, Affine::IDENTITY, RED, None, &r);
}

fn render(scene: &Scene, name: &str) -> Option<Vec<u8>> {
    let (device, queue, mut r) = setup(name)?;
    Some(render_scene(&device, &queue, &mut r, scene, (64, 64), Color::WHITE))
}

/// L12 AC5: one draw under opacity folds into its color (no layer target).
#[test]
#[ignore = "gpu"]
fn opacity_single_draw_is_folded() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    layer(&mut p, 0.5, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 64.0));
    red(&mut p, Rect::new(4.0, 4.0, 40.0, 40.0));
    p.pop_layer();
    let Some(rgba) = render(&s, "opacity_single_draw_is_folded") else { return };
    let c = pixel(&rgba, 64, 20, 20);
    assert!(near(c, PINK, 2), "{c:?}");
    assert!(is_white(pixel(&rgba, 64, 50, 50)));
}

/// L13 AC5 / L14 AC6: two overlapping fills under alpha 0.5 composite once — the overlap is
/// the same pink as the single-covered area (no double blend).
#[test]
#[ignore = "gpu"]
fn opacity_group_does_not_double_blend() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    layer(&mut p, 0.5, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 64.0));
    red(&mut p, Rect::new(4.0, 4.0, 40.0, 40.0));
    red(&mut p, Rect::new(20.0, 20.0, 60.0, 60.0));
    p.pop_layer();
    let Some(rgba) = render(&s, "opacity_group_does_not_double_blend") else { return };
    for (x, y) in [(12, 12), (32, 32), (50, 50)] {
        let c = pixel(&rgba, 64, x, y);
        assert!(near(c, PINK, 2), "({x},{y}) {c:?}");
    }
    assert!(is_white(pixel(&rgba, 64, 2, 2)));
    assert!(is_white(pixel(&rgba, 64, 50, 10)));
}

/// L14 AC6: the layer's own circle clip applies once, at the composite.
#[test]
#[ignore = "gpu"]
fn opacity_layer_with_circle_clip() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    layer(&mut p, 0.5, Affine::IDENTITY, &kurbo::Circle::new((32.0, 32.0), 20.0));
    red(&mut p, Rect::new(0.0, 0.0, 40.0, 64.0));
    red(&mut p, Rect::new(24.0, 0.0, 64.0, 64.0));
    p.pop_layer();
    let Some(rgba) = render(&s, "opacity_layer_with_circle_clip") else { return };
    for y in 0..64u32 {
        for x in 0..64u32 {
            let d = ((f64::from(x) + 0.5 - 32.0).powi(2) + (f64::from(y) + 0.5 - 32.0).powi(2)).sqrt();
            let c = pixel(&rgba, 64, x, y);
            if d < 19.0 {
                assert!(near(c, PINK, 2), "({x},{y}) {c:?}");
            } else if d > 21.0 {
                assert!(is_white(c), "({x},{y}) {c:?}");
            }
        }
    }
}

/// L14 AC6: a layer inside a rotated (ancestor) clip: the clip applies at the composite only.
#[test]
#[ignore = "gpu"]
fn layer_inside_rotated_clip() {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    let rot = Affine::rotate_about(std::f64::consts::FRAC_PI_6, (32.0, 32.0));
    let clip = Rect::new(16.0, 16.0, 48.0, 48.0);
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, rot, &clip);
    layer(&mut p, 0.5, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 64.0));
    red(&mut p, Rect::new(0.0, 0.0, 40.0, 64.0));
    red(&mut p, Rect::new(24.0, 0.0, 64.0, 64.0));
    p.pop_layer();
    p.pop_layer();
    let Some(rgba) = render(&s, "layer_inside_rotated_clip") else { return };
    let inv = rot.inverse();
    for y in 0..64u32 {
        for x in 0..64u32 {
            let q = inv * kurbo::Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
            let c = pixel(&rgba, 64, x, y);
            if clip.inset(-1.5).contains(q) {
                assert!(near(c, PINK, 2), "({x},{y}) {c:?}");
            } else if !clip.inset(1.5).contains(q) {
                assert!(is_white(c), "({x},{y}) {c:?}");
            }
        }
    }
}

/// L13 AC6: a mask clip inside the layer target masks only the layer's content.
#[test]
#[ignore = "gpu"]
fn layer_with_mask_inside() {
    let mut star = kurbo::BezPath::new();
    star.move_to((32.0, 2.0));
    star.line_to((50.0, 60.0));
    star.line_to((4.0, 22.0));
    star.line_to((60.0, 22.0));
    star.line_to((14.0, 60.0));
    star.close_path();
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    layer(&mut p, 0.5, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 64.0));
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &star);
    red(&mut p, Rect::new(0.0, 0.0, 64.0, 64.0));
    p.pop_layer();
    red(&mut p, Rect::new(0.0, 56.0, 8.0, 64.0)); // unmasked layer content
    p.pop_layer();
    let Some(rgba) = render(&s, "layer_with_mask_inside") else { return };
    assert!(near(pixel(&rgba, 64, 32, 30), PINK, 2), "star body");
    assert!(near(pixel(&rgba, 64, 3, 60), PINK, 2), "unmasked fill");
    assert!(is_white(pixel(&rgba, 64, 4, 4)), "outside the star");
    assert!(is_white(pixel(&rgba, 64, 32, 62)), "between the star's feet");
}
