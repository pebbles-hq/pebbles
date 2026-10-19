//! The clip stack end to end (09 §2, L01): every clip route (scissor, SDF, Tier B mask, GPU
//! mask) on 64×64 white, fill = full-canvas `#FF0000` unless stated. Probes are analytic.

use super::*;
use kurbo::Shape as _;
use peniko::Mix;

const RED: Color = Color::from_rgba8(255, 0, 0, 255);
const RED_PX: [u8; 4] = [255, 0, 0, 255];

fn near(c: [u8; 4], want: [u8; 3], tol: i32) -> bool {
    (0..3).all(|i| (i32::from(c[i]) - i32::from(want[i])).abs() <= tol)
}

fn clip(p: &mut Painter<'_>, fill: Fill, xf: Affine, shape: &impl kurbo::Shape) {
    p.push_layer(fill, Mix::Normal, 1.0, xf, shape);
}

fn fill_canvas(p: &mut Painter<'_>) {
    p.fill(Fill::NonZero, Affine::IDENTITY, RED, None, &Rect::new(0.0, 0.0, 64.0, 64.0));
}

/// Render `build` at 64×64 on white.
fn draw(name: &str, build: impl FnOnce(&mut Painter<'_>)) -> Option<Vec<u8>> {
    let (device, queue, mut r) = setup(name)?;
    let mut s = Scene::new();
    build(&mut Painter::new(&mut s));
    Some(render_scene(&device, &queue, &mut r, &s, (64, 64), Color::WHITE))
}

/// A pentagram centered at `c` with outer radius `r` (self-intersecting: NonZero fills the
/// center pentagon, EvenOdd leaves it empty).
fn pentagram(c: (f64, f64), r: f64) -> kurbo::BezPath {
    let mut path = kurbo::BezPath::new();
    for i in 0..5 {
        let a = -std::f64::consts::FRAC_PI_2 + f64::from(i) * 4.0 * std::f64::consts::PI / 5.0;
        let pt = (c.0 + r * a.cos(), c.1 + r * a.sin());
        if i == 0 { path.move_to(pt) } else { path.line_to(pt) }
    }
    path.close_path();
    path
}

#[test]
#[ignore = "gpu"]
fn rect_clip_is_pixel_exact() {
    let Some(px) = draw("rect_clip_is_pixel_exact", |p| {
        clip(p, Fill::NonZero, Affine::IDENTITY, &Rect::new(16.0, 16.0, 48.0, 48.0));
        fill_canvas(p);
        p.pop_layer();
    }) else {
        return;
    };
    assert_eq!(pixel(&px, 64, 15, 15), [255; 4]);
    assert_eq!(pixel(&px, 64, 48, 48), [255; 4]);
    assert_eq!(pixel(&px, 64, 16, 16), RED_PX);
    assert_eq!(pixel(&px, 64, 47, 47), RED_PX);
}

#[test]
#[ignore = "gpu"]
fn fractional_rect_clip_is_antialiased() {
    let Some(px) = draw("fractional_rect_clip_is_antialiased", |p| {
        clip(p, Fill::NonZero, Affine::IDENTITY, &Rect::new(16.5, 16.0, 48.0, 48.0));
        fill_canvas(p);
        p.pop_layer();
    }) else {
        return;
    };
    let c = pixel(&px, 64, 16, 32);
    assert!(near(c, [255, 128, 128], 3), "{c:?}");
    assert_eq!(pixel(&px, 64, 17, 32), RED_PX);
    assert!(is_white(pixel(&px, 64, 15, 32)));
}

#[test]
#[ignore = "gpu"]
fn circle_clip_avatar() {
    let Some(px) = draw("circle_clip_avatar", |p| {
        clip(p, Fill::NonZero, Affine::IDENTITY, &kurbo::Circle::new((32.0, 32.0), 24.0));
        fill_canvas(p);
        p.pop_layer();
    }) else {
        return;
    };
    assert_eq!(pixel(&px, 64, 32, 32), RED_PX);
    assert_eq!(pixel(&px, 64, 32, 10), RED_PX);
    assert!(is_white(pixel(&px, 64, 32, 4)));
    assert!(is_white(pixel(&px, 64, 12, 12)));
}

#[test]
#[ignore = "gpu"]
fn rrect_clip_cuts_corners() {
    let Some(px) = draw("rrect_clip_cuts_corners", |p| {
        clip(p, Fill::NonZero, Affine::IDENTITY, &kurbo::RoundedRect::new(8.0, 8.0, 56.0, 56.0, 16.0));
        fill_canvas(p);
        p.pop_layer();
    }) else {
        return;
    };
    assert!(is_white(pixel(&px, 64, 9, 9)));
    assert_eq!(pixel(&px, 64, 32, 9), RED_PX);
    assert_eq!(pixel(&px, 64, 24, 24), RED_PX);
}

fn star_clip(name: &str, fill: Fill) -> Option<Vec<u8>> {
    draw(name, |p| {
        clip(p, fill, Affine::IDENTITY, &pentagram((32.0, 32.0), 28.0));
        fill_canvas(p);
        p.pop_layer();
    })
}

#[test]
#[ignore = "gpu"]
fn star_clip_nonzero_fills_center() {
    let Some(px) = star_clip("star_clip_nonzero_fills_center", Fill::NonZero) else { return };
    assert_eq!(pixel(&px, 64, 32, 32), RED_PX);
    assert_eq!(pixel(&px, 64, 32, 8), RED_PX);
}

#[test]
#[ignore = "gpu"]
fn star_clip_evenodd_leaves_center() {
    let Some(px) = star_clip("star_clip_evenodd_leaves_center", Fill::EvenOdd) else { return };
    assert!(is_white(pixel(&px, 64, 32, 32)));
    assert_eq!(pixel(&px, 64, 32, 8), RED_PX);
}

#[test]
#[ignore = "gpu"]
fn rotated_rect_clip() {
    let Some(px) = draw("rotated_rect_clip", |p| {
        let rot = Affine::rotate_about(std::f64::consts::FRAC_PI_4, (32.0, 32.0));
        clip(p, Fill::NonZero, rot, &Rect::new(16.0, 16.0, 48.0, 48.0));
        fill_canvas(p);
        p.pop_layer();
    }) else {
        return;
    };
    assert_eq!(pixel(&px, 64, 32, 32), RED_PX);
    assert!(is_white(pixel(&px, 64, 17, 17)));
    assert_eq!(pixel(&px, 64, 32, 12), RED_PX);
}

#[test]
#[ignore = "gpu"]
fn nested_circles_intersect() {
    let Some(px) = draw("nested_circles_intersect", |p| {
        clip(p, Fill::NonZero, Affine::IDENTITY, &kurbo::Circle::new((24.0, 32.0), 16.0));
        clip(p, Fill::NonZero, Affine::IDENTITY, &kurbo::Circle::new((40.0, 32.0), 16.0));
        fill_canvas(p);
        p.pop_layer();
        p.pop_layer();
    }) else {
        return;
    };
    assert_eq!(pixel(&px, 64, 32, 32), RED_PX);
    assert!(is_white(pixel(&px, 64, 12, 32)));
    assert!(is_white(pixel(&px, 64, 52, 32)));
}

#[test]
#[ignore = "gpu"]
fn clip_pop_restores_parent() {
    let Some(px) = draw("clip_pop_restores_parent", |p| {
        clip(p, Fill::NonZero, Affine::IDENTITY, &kurbo::Circle::new((32.0, 32.0), 24.0));
        clip(p, Fill::NonZero, Affine::IDENTITY, &Rect::new(28.0, 28.0, 36.0, 36.0));
        p.pop_layer();
        fill_canvas(p);
        p.pop_layer();
    }) else {
        return;
    };
    assert_eq!(pixel(&px, 64, 20, 32), RED_PX);
    assert!(is_white(pixel(&px, 64, 4, 4)));
}

/// One draw of each kind, in its own quadrant of a `size` canvas.
fn every_kind(p: &mut Painter<'_>, size: f64) {
    let h = size / 2.0;
    let q = |x: f64, y: f64| Rect::new(x, y, x + h, y + h);
    p.fill(Fill::NonZero, Affine::IDENTITY, RED, None, &q(0.0, 0.0));
    let rr = q(h, 0.0).inset(-2.0).to_rounded_rect(h / 8.0);
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgba8(0, 128, 255, 255), None, &rr);
    let img = rgba_image(4, 4, [20, 160, 60, 255]);
    p.draw_image(&peniko::ImageBrush::new(img), Affine::translate((0.0, h)) * Affine::scale(h / 4.0));
    p.draw_blurred_rounded_rect(Affine::IDENTITY, q(h, h).inset(-h / 6.0), Color::BLACK, 4.0, 2.0);
    let font =
        peniko::FontData::new(peniko::Blob::from(pebbles_render::fonts::builtin_fonts()[0].1.to_vec()), 0);
    let gid = u32::from(swash::FontRef::from_index(font.data.data(), 0).expect("font").charmap().map('M'));
    let glyphs =
        (0..4).map(|i| Glyph { id: gid, x: (h * 0.1 + f64::from(i) * h * 0.22) as f32, y: (h * 0.8) as f32 });
    p.draw_glyphs(
        &font,
        (h * 0.35) as f32,
        &[],
        &Brush::Solid(Color::BLACK),
        Affine::translate((h / 2.0, 0.0)),
        None,
        Fill::NonZero,
        glyphs,
    );
}

/// Differential oracle: under `clip`, pixels at least 1.5 px inside equal the unclipped
/// render; pixels at least 1.5 px outside are white.
fn assert_clip_honored(name: &str, size: u32, xf: Affine, shape: &kurbo::BezPath, fill: Fill) {
    let Some((device, queue, mut r)) = setup(name) else { return };
    let render = |r: &mut Renderer, clipped: bool| {
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        if clipped {
            p.push_layer(fill, Mix::Normal, 1.0, xf, shape);
        }
        every_kind(&mut p, f64::from(size));
        if clipped {
            p.pop_layer();
        }
        render_scene(&device, &queue, r, &s, (size, size), Color::WHITE)
    };
    let plain = render(&mut r, false);
    let clipped = render(&mut r, true);
    let device_path = xf * shape.clone();
    let inside = |x: f64, y: f64| {
        let w = device_path.winding(kurbo::Point::new(x, y));
        if fill == Fill::EvenOdd { w % 2 != 0 } else { w != 0 }
    };
    let (mut n_in, mut n_out) = (0, 0);
    for y in (0..size).step_by(3) {
        for x in (0..size).step_by(3) {
            let (cx, cy) = (f64::from(x) + 0.5, f64::from(y) + 0.5);
            let ring: Vec<bool> = (0..8)
                .map(|k| {
                    let a = f64::from(k) * std::f64::consts::FRAC_PI_4;
                    inside(cx + 1.5 * a.cos(), cy + 1.5 * a.sin())
                })
                .chain(std::iter::once(inside(cx, cy)))
                .collect();
            let c = pixel(&clipped, size, x, y);
            if ring.iter().all(|&b| b) {
                n_in += 1;
                let want = pixel(&plain, size, x, y);
                assert!(
                    near(c, [want[0], want[1], want[2]], 2),
                    "{name} inside ({x},{y}): {c:?} vs {want:?}"
                );
            } else if ring.iter().all(|&b| !b) {
                n_out += 1;
                assert!(is_white(c), "{name} outside ({x},{y}): {c:?}");
            }
        }
    }
    assert!(n_in > 50 && n_out > 50, "{name}: probes {n_in} in / {n_out} out");
}

#[test]
#[ignore = "gpu"]
fn every_draw_kind_honors_mask_clip() {
    // Tier B: a rotated star.
    let star = pentagram((32.0, 32.0), 30.0);
    let rot = Affine::rotate_about(0.2, (32.0, 32.0));
    assert_clip_honored("every_draw_kind_honors_mask_clip/tier_b", 64, rot, &star, Fill::NonZero);
    // GPU mask (row 9): a rotated rect larger than MASK_MAX_AREA on 640×640.
    let big = Rect::new(60.0, 60.0, 580.0, 580.0).to_path(0.1);
    let rot = Affine::rotate_about(0.4, (320.0, 320.0));
    assert_clip_honored("every_draw_kind_honors_mask_clip/gpu", 640, rot, &big, Fill::NonZero);
}

#[test]
#[ignore = "gpu"]
fn deep_clip_nesting_is_bounded() {
    let Some(px) = draw("deep_clip_nesting_is_bounded", |p| {
        let rot = Affine::rotate_about(0.3, (32.0, 32.0));
        for _ in 0..300 {
            clip(p, Fill::NonZero, rot, &Rect::new(16.0, 16.0, 48.0, 48.0));
        }
        fill_canvas(p);
        for _ in 0..300 {
            p.pop_layer();
        }
    }) else {
        return;
    };
    assert_eq!(pixel(&px, 64, 32, 32), RED_PX);
    assert!(is_white(pixel(&px, 64, 2, 2)));
}
