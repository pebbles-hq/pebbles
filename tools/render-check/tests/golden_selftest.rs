//! Oracle self-validation (V44, 05 §6.7a): the golden model reproduces closed-form results.

use kurbo::{Affine, BezPath, Circle, Point, Rect, Shape, Stroke};
use pebbles_render::paint::display_list::Geometry;
use peniko::Fill;
use render_check::golden::coverage::fill_coverage;
use render_check::golden::shapes::{Snap, fill_geometry, stroke_geometry};

const W: u32 = 48;
const H: u32 = 48;

fn at(cov: &[f64], x: u32, y: u32) -> f64 {
    cov[(y * W + x) as usize]
}

fn overlap(a0: f64, a1: f64, i: f64) -> f64 {
    (a1.min(i + 1.0) - a0.max(i)).max(0.0)
}

fn rect_path(r: Rect) -> BezPath {
    r.to_path(0.0)
}

#[test]
fn rect_fractional_offsets_exact() {
    // Horizontal edges anywhere; vertical edges on the 1/256 sub-scanline grid: exact.
    let r = Rect::new(3.3, 5.0 + 77.0 / 256.0, 20.71, 17.0 + 3.0 / 256.0);
    let cov = fill_coverage(&rect_path(r), Affine::IDENTITY, Fill::NonZero, W, H);
    for y in 0..H {
        for x in 0..W {
            let want = overlap(r.x0, r.x1, f64::from(x)) * overlap(r.y0, r.y1, f64::from(y));
            assert!((at(&cov, x, y) - want).abs() < 1e-12, "({x},{y}) {} vs {want}", at(&cov, x, y));
        }
    }
    // Arbitrary vertical offsets: within the documented 1/512.
    let r = Rect::new(2.25, 4.3, 11.9, 9.77);
    let cov = fill_coverage(&rect_path(r), Affine::IDENTITY, Fill::NonZero, W, H);
    for y in 0..H {
        for x in 0..W {
            let want = overlap(r.x0, r.x1, f64::from(x)) * overlap(r.y0, r.y1, f64::from(y));
            assert!((at(&cov, x, y) - want).abs() <= 1.0 / 512.0 + 1e-12);
        }
    }
}

#[test]
fn circle_area() {
    let c = Circle::new((24.3, 23.7), 17.5);
    let g = Geometry::Circle(c);
    let cov = fill_geometry(&g, Affine::IDENTITY, Fill::NonZero, Snap::Auto, W, H);
    let sum: f64 = cov.iter().sum();
    let area = std::f64::consts::PI * 17.5 * 17.5;
    assert!(((sum - area) / area).abs() < 1e-4, "{sum} vs {area}");
}

/// Area of the unit pixel `(x, y)` on the side `n · p ≤ d` (Sutherland–Hodgman clip).
fn half_plane_area(x: f64, y: f64, n: (f64, f64), d: f64) -> f64 {
    let poly =
        [Point::new(x, y), Point::new(x + 1.0, y), Point::new(x + 1.0, y + 1.0), Point::new(x, y + 1.0)];
    let f = |p: Point| n.0 * p.x + n.1 * p.y - d;
    let mut out = Vec::new();
    for i in 0..4 {
        let (a, b) = (poly[i], poly[(i + 1) % 4]);
        let (fa, fb) = (f(a), f(b));
        if fa <= 0.0 {
            out.push(a);
        }
        if (fa < 0.0) != (fb < 0.0) && fa != fb {
            out.push(a + (b - a) * (fa / (fa - fb)));
        }
    }
    let mut area = 0.0;
    for i in 0..out.len() {
        let (a, b) = (out[i], out[(i + 1) % out.len()]);
        area += a.x * b.y - b.x * a.y;
    }
    area.abs() / 2.0
}

#[test]
fn half_plane_angles() {
    for deg in [0.0f64, 30.0, 45.0, 60.0] {
        let th = deg.to_radians();
        let n = (-th.sin(), th.cos()); // the edge direction is (cos, sin); fill the n·p ≤ d side
        let c = Point::new(24.0, 24.0);
        let d = n.0 * c.x + n.1 * c.y;
        // A huge polygon whose only edge crossing the target is the half-plane boundary.
        let dir = kurbo::Vec2::new(th.cos(), th.sin()) * 1000.0;
        let away = kurbo::Vec2::new(-n.0, -n.1) * 1000.0;
        let mut p = BezPath::new();
        p.move_to(c - dir);
        p.line_to(c + dir);
        p.line_to(c + dir + away);
        p.line_to(c - dir + away);
        p.close_path();
        let cov = fill_coverage(&p, Affine::IDENTITY, Fill::NonZero, W, H);
        for y in 0..H {
            for x in 0..W {
                let want = half_plane_area(f64::from(x), f64::from(y), n, d);
                let got = at(&cov, x, y);
                assert!((got - want).abs() < 1e-3, "{deg}° ({x},{y}) {got} vs {want}");
            }
        }
    }
}

#[test]
fn evenodd_overlap_exact() {
    let mut p = rect_path(Rect::new(4.0, 4.0, 20.0, 20.0));
    p.extend(rect_path(Rect::new(12.0, 12.0, 30.0, 30.0)));
    let eo = fill_coverage(&p, Affine::IDENTITY, Fill::EvenOdd, W, H);
    let nz = fill_coverage(&p, Affine::IDENTITY, Fill::NonZero, W, H);
    assert_eq!(at(&eo, 15, 15), 0.0);
    assert_eq!(at(&nz, 15, 15), 1.0);
    assert_eq!(at(&eo, 5, 5), 1.0);
    assert_eq!(at(&eo, 25, 25), 1.0);
    assert_eq!(at(&eo, 35, 35), 0.0);
}

#[test]
fn nonzero_opposite_winding_hole_exact() {
    let outer = Rect::new(4.0, 4.0, 40.0, 40.0);
    let inner = Rect::new(10.5, 10.5, 30.25, 30.25);
    let reversed = |r: Rect| {
        let mut p = BezPath::new();
        p.move_to((r.x0, r.y0));
        p.line_to((r.x0, r.y1));
        p.line_to((r.x1, r.y1));
        p.line_to((r.x1, r.y0));
        p.close_path();
        p
    };
    let mut hole = rect_path(outer);
    hole.extend(reversed(inner));
    let cov = fill_coverage(&hole, Affine::IDENTITY, Fill::NonZero, W, H);
    for y in 0..H {
        for x in 0..W {
            let (fx, fy) = (f64::from(x), f64::from(y));
            let o = overlap(outer.x0, outer.x1, fx) * overlap(outer.y0, outer.y1, fy);
            let i = overlap(inner.x0, inner.x1, fx) * overlap(inner.y0, inner.y1, fy);
            assert!((at(&cov, x, y) - (o - i)).abs() < 1e-12, "({x},{y})");
        }
    }
    // Same direction: winding 2 inside, still covered.
    let mut same = rect_path(outer);
    same.extend(rect_path(inner));
    let cov = fill_coverage(&same, Affine::IDENTITY, Fill::NonZero, W, H);
    assert_eq!(at(&cov, 20, 20), 1.0);
}

#[test]
fn snapped_border_is_whole_pixels() {
    let g = Geometry::Rect(Rect::new(5.3, 6.6, 30.2, 25.45));
    for width in [1.0, 1.4, 2.0, 2.6, 3.0] {
        let style = Stroke::new(width).with_join(kurbo::Join::Miter);
        let cov = stroke_geometry(&g, &style, Affine::IDENTITY, Snap::Auto, W, H);
        assert!(cov.iter().all(|&c| c.abs() < 1e-12 || (c - 1.0).abs() < 1e-12), "width {width}");
        assert!(cov.iter().any(|&c| c > 0.5));
    }
    let fill = fill_geometry(&g, Affine::scale(1.5), Fill::NonZero, Snap::Auto, W, H);
    assert!(fill.iter().all(|&c| c == 0.0 || (c - 1.0).abs() < 1e-12));
    // Snap::Off keeps the fractional edges.
    let off = fill_geometry(&g, Affine::IDENTITY, Fill::NonZero, Snap::Off, W, H);
    assert!(off.iter().any(|&c| c > 0.01 && c < 0.99));
}

#[test]
fn render_composites_premultiplied() {
    use pebbles_render::paint::{Color, Painter, Scene};
    use render_check::golden::Golden;
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.set_pixel_snap(pebbles_render::paint::display_list::PixelSnap::Off);
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(255, 0, 0, 128),
        None,
        &Rect::new(2.0, 2.0, 6.5, 6.0),
    );
    let img = Golden::render(&scene, 8, 8, Color::WHITE).expect("solid fill is supported");
    // 0.502·red over white: G = B = 1 − 0.502 → 127; the half-covered column halves the alpha.
    assert_eq!(img.px(3, 3), [255, 127, 127, 255]);
    assert_eq!(img.px(6, 3), [255, 191, 191, 255]);
    assert_eq!(img.px(7, 7), [255, 255, 255, 255]);
    // The same rect with the default PixelSnap::Auto: x1 = 6.5 rounds half up to 7.
    let mut snapped = Scene::new();
    Painter::new(&mut snapped).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(255, 0, 0, 128),
        None,
        &Rect::new(2.0, 2.0, 6.5, 6.0),
    );
    let img = Golden::render(&snapped, 8, 8, Color::WHITE).expect("supported");
    assert_eq!(img.px(6, 3), [255, 127, 127, 255]);
    // Gradients are judged since V45 (previously reported as GoldenUnsupported).
    let mut scene = Scene::new();
    let grad =
        Gradient::new_linear((0.0, 0.0), (8.0, 0.0)).with_stops([Color::BLACK, Color::WHITE].as_slice());
    Painter::new(&mut scene).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        &Brush::Gradient(grad),
        None,
        &Rect::new(1.0, 1.0, 5.0, 5.0),
    );
    assert!(Golden::render(&scene, 8, 8, Color::WHITE).is_ok());
}

// ---- V45: paint, compose, clips and layers ----

use pebbles_render::paint::{Painter, Scene};
use peniko::{BlendMode, Brush, Color, Compose, Extend, Gradient, Mix};
use render_check::golden::Golden;
use render_check::golden::compose::composite;
use render_check::golden::paint::{Paint, Texels, apply_extend};

fn gradient_paint(g: Gradient) -> Paint {
    Paint::new(&Brush::Gradient(g), Affine::IDENTITY, 1.0).unwrap().unwrap()
}

fn close(a: [f64; 4], b: [f64; 4], tol: f64) -> bool {
    a.iter().zip(&b).all(|(x, y)| (x - y).abs() <= tol)
}

#[test]
fn uniform_gradient_is_flat() {
    let red = Color::from_rgba8(200, 30, 40, 255);
    let g = Gradient::new_linear((0.0, 0.0), (10.0, 0.0)).with_stops([red, red].as_slice());
    let p = gradient_paint(g);
    let want = p.at(0.5, 0.5);
    for x in 0..20 {
        assert!(close(p.at(f64::from(x) + 0.5, 3.5), want, 1e-12));
    }
}

#[test]
fn linear_gradient_midpoint() {
    let g = Gradient::new_linear((0.0, 0.0), (10.0, 0.0)).with_stops([Color::BLACK, Color::WHITE].as_slice());
    let c = gradient_paint(g).at(5.0, 0.0);
    assert!(close(c, [0.5, 0.5, 0.5, 1.0], 1e-9), "{c:?}");
}

#[test]
fn pad_repeat_reflect_periods() {
    for t in [-1.25, -0.5, 0.0, 0.3, 0.999, 1.0, 1.7, 2.4, 3.6] {
        let pad = apply_extend(t, Extend::Pad);
        assert!((pad - t.clamp(0.0, 1.0)).abs() < 1e-12);
        let rep = apply_extend(t, Extend::Repeat);
        assert!((rep - apply_extend(t + 1.0, Extend::Repeat)).abs() < 1e-9, "repeat period 1 at {t}");
        let refl = apply_extend(t, Extend::Reflect);
        assert!((refl - apply_extend(t + 2.0, Extend::Reflect)).abs() < 1e-9, "reflect period 2 at {t}");
        assert!((refl - apply_extend(-t, Extend::Reflect)).abs() < 1e-9, "reflect is even at {t}");
        assert!((0.0..=1.0).contains(&refl));
    }
}

#[test]
fn bilinear_2x2_center_is_average() {
    let px: Vec<u8> = [[255, 0, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255], [255, 255, 255, 255]].concat();
    let data = peniko::ImageData {
        data: peniko::Blob::from(px),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::Alpha,
        width: 2,
        height: 2,
    };
    let c = Texels::of(&data).sample(1.0, 1.0, Extend::Pad, Extend::Pad, false);
    assert!(close(c, [0.5, 0.5, 0.5, 1.0], 1e-12), "{c:?}");
}

#[test]
fn multiply_white_is_identity() {
    let s = [0.3, 0.12, 0.05, 0.6];
    let white = [1.0, 1.0, 1.0, 1.0];
    let normal = composite(white, s, Mix::Normal, Compose::SrcOver);
    assert!(close(composite(white, s, Mix::Multiply, Compose::SrcOver), normal, 1e-12));
}

#[test]
fn screen_black_is_identity() {
    let s = [0.3, 0.12, 0.05, 0.6];
    let black = [0.0, 0.0, 0.0, 1.0];
    let normal = composite(black, s, Mix::Normal, Compose::SrcOver);
    assert!(close(composite(black, s, Mix::Screen, Compose::SrcOver), normal, 1e-12));
}

fn render(f: impl FnOnce(&mut Painter<'_>)) -> render_check::metric::Rgba8 {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.set_pixel_snap(pebbles_render::paint::display_list::PixelSnap::Off);
    f(&mut p);
    Golden::render(&scene, 16, 16, Color::WHITE).expect("supported")
}

#[test]
fn opacity_layer_equals_alpha_scaled_content() {
    let (a, b) = (Rect::new(1.0, 1.0, 7.0, 7.0), Rect::new(9.0, 9.0, 15.0, 15.0));
    let (ca, cb) = (Color::from_rgba8(200, 20, 20, 255), Color::from_rgba8(20, 20, 200, 255));
    let layered = render(|p| {
        p.push_layer(
            Fill::NonZero,
            BlendMode::default(),
            0.5,
            Affine::IDENTITY,
            &Rect::new(0.0, 0.0, 16.0, 16.0),
        );
        p.fill(Fill::NonZero, Affine::IDENTITY, ca, None, &a);
        p.fill(Fill::NonZero, Affine::IDENTITY, cb, None, &b);
        p.pop_layer();
    });
    let scaled = render(|p| {
        p.fill(Fill::NonZero, Affine::IDENTITY, ca.multiply_alpha(0.5), None, &a);
        p.fill(Fill::NonZero, Affine::IDENTITY, cb.multiply_alpha(0.5), None, &b);
    });
    assert_eq!(layered.data, scaled.data);
}

#[test]
fn clip_path_equals_coverage_product() {
    let clip = Circle::new((8.0, 8.0), 6.3);
    let rect = Rect::new(2.25, 3.5, 13.75, 12.0);
    let red = Color::from_rgba8(255, 0, 0, 255);
    let img = render(|p| {
        p.push_layer(Fill::NonZero, BlendMode::default(), 1.0, Affine::IDENTITY, &clip);
        p.fill(Fill::NonZero, Affine::IDENTITY, red, None, &rect);
        p.pop_layer();
    });
    let shape = |g| fill_geometry(&g, Affine::IDENTITY, Fill::NonZero, Snap::Off, 16, 16);
    let cc = shape(Geometry::Path(clip.to_path(0.01)));
    let rc = shape(Geometry::Rect(rect));
    for y in 0..16u32 {
        for x in 0..16u32 {
            let k = cc[(y * 16 + x) as usize] * rc[(y * 16 + x) as usize];
            // Red over white with coverage k: G = B = 1 − k.
            let want = ((1.0 - k).clamp(0.0, 1.0) * 255.0 + 0.5).floor() as u8;
            let got = img.px(x, y);
            assert!(got[1].abs_diff(want) <= 1, "({x},{y}) {got:?} want G {want}");
        }
    }
}

// ---- V46: shadows and glyph outlines ----

use render_check::golden::shadow::shadow_coverage;

#[test]
fn sigma_to_zero_equals_rect() {
    let r = Rect::new(8.0, 10.0, 30.0, 26.0);
    let exact = fill_coverage(&rect_path(r), Affine::IDENTITY, Fill::NonZero, W, H);
    for sigma in [0.0, 1e-4] {
        let s = shadow_coverage(Affine::IDENTITY, r, 0.0, sigma, W, H);
        let max = s.iter().zip(&exact).map(|(a, b)| (a - b).abs()).fold(0.0, f64::max);
        assert!(max < 1e-6, "σ = {sigma}: max |shadow − rect| = {max}");
    }
}

#[test]
fn wide_shadow_center_value() {
    // A 200×200 shadow (σ = 4): the center is fully covered; a pixel centered on an edge line
    // averages ½(1 + erf(·)) symmetrically about the edge, i.e. 0.5 (closed form).
    let (w, h) = (240, 240);
    let r = Rect::new(20.0, 20.0, 220.5, 220.0);
    let s = shadow_coverage(Affine::IDENTITY, r, 0.0, 4.0, w, h);
    let at = |x: u32, y: u32| s[(y * w + x) as usize];
    assert!((at(120, 120) - 1.0).abs() < 1e-6, "center {}", at(120, 120));
    assert!((at(220, 120) - 0.5).abs() < 2e-3, "edge {}", at(220, 120));
    assert!(at(239, 120) < 1e-3, "beyond 4σ: {}", at(239, 120));
}

#[test]
fn glyph_outline_area_matches_mask_sum() {
    use swash::scale::{Render, ScaleContext, Source};
    use swash::zeno::Format;
    let font =
        peniko::FontData::new(peniko::Blob::from(pebbles_render::fonts::builtin_fonts()[0].1.to_vec()), 0);
    let face = swash::FontRef::from_index(font.data.data(), font.index as usize).expect("Inter");
    for hint in [false, true] {
        for ch in ['H', 'o', 'g'] {
            let id = face.charmap().map(ch);
            let golden = render_check::golden::text::glyph_coverage(
                &font,
                id,
                16.0,
                hint,
                Point::new(16.0, 32.0),
                48,
                48,
            )
            .expect("outline");
            let gold: f64 = golden.iter().sum();
            let mut ctx = ScaleContext::new();
            let mut scaler = ctx.builder(face).size(16.0).hint(hint).build();
            let img =
                Render::new(&[Source::Outline]).format(Format::Alpha).render(&mut scaler, id).expect("mask");
            let mask: f64 = img.data.iter().map(|&v| f64::from(v) / 255.0).sum();
            // swash's own mask renderer subdivides curves coarsely and loses area on small
            // glyphs (≈ 1 % on a 16-px 'o'); the renderer rasterizes outlines through the Tier B
            // pipeline instead, so swash's mask gets a looser bound and the renderer raster 0.5 %.
            let rel = (gold - mask).abs() / mask;
            assert!(
                rel < 0.015,
                "{ch} hint={hint}: golden {gold:.2} vs swash mask {mask:.2} ({:.3} %)",
                rel * 100.0
            );
            let outline = scaler.scale_outline(id).expect("outline");
            let (pts, mut path, mut i) = (outline.points(), BezPath::new(), 0);
            let q = |k: usize| Point::new(16.0 + f64::from(pts[k].x), 32.0 - f64::from(pts[k].y));
            for verb in outline.verbs() {
                use swash::zeno::Verb;
                match verb {
                    Verb::MoveTo => path.move_to(q(i)),
                    Verb::LineTo => path.line_to(q(i)),
                    Verb::QuadTo => path.quad_to(q(i), q(i + 1)),
                    Verb::CurveTo => path.curve_to(q(i), q(i + 1), q(i + 2)),
                    Verb::Close => path.close_path(),
                }
                i += match verb {
                    Verb::MoveTo | Verb::LineTo => 1,
                    Verb::QuadTo => 2,
                    Verb::CurveTo => 3,
                    Verb::Close => 0,
                };
            }
            let raster = pebbles_gpu::__tier_b_mask_for_check(&path, Affine::IDENTITY, Fill::NonZero, 48, 48);
            let tier_b: f64 = raster.iter().map(|&v| f64::from(v) / 255.0).sum();
            let rel = (gold - tier_b).abs() / tier_b;
            assert!(
                rel < 0.005,
                "{ch} hint={hint}: golden {gold:.2} vs renderer raster {tier_b:.2} ({:.3} %)",
                rel * 100.0
            );
        }
    }
}

// ---- V51 (steps 4, 4b): bitmap-glyph resampling and COLR glyphs ----

fn colr_font() -> peniko::FontData {
    let bytes =
        std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/fonts/colr-test.ttf")).expect("colr-test.ttf");
    peniko::FontData::new(peniko::Blob::from(bytes), 0)
}

/// Glyph `ch` of the COLR test font, 100 px, pen at (10, 110), over white on 120×120.
fn colr_render(ch: char, color: Color) -> render_check::metric::Rgba8 {
    let font = colr_font();
    let gid = {
        let f = swash::FontRef::from_index(font.data.data(), 0).unwrap();
        u32::from(f.charmap().map(ch))
    };
    let mut scene = Scene::new();
    let g = [pebbles_render::paint::Glyph { id: gid, x: 10.0, y: 110.0 }];
    Painter::new(&mut scene).draw_glyphs(
        &font,
        100.0,
        &[],
        &Brush::Solid(color),
        Affine::IDENTITY,
        None,
        Fill::NonZero,
        g.into_iter(),
    );
    Golden::render(&scene, 120, 120, Color::WHITE).expect("COLR is supported")
}

#[test]
fn colr_v0_layers_composite_in_order() {
    // Glyph A: red square (font 100..700) then blue circle (r 280 at 500,500): blue on top.
    let img = colr_render('A', Color::BLACK);
    // Font (500, 500) → device (10 + 50, 110 − 50); center of both layers.
    assert_eq!(img.px(60, 60), [0, 0, 255, 255], "blue over red where they overlap");
    // Font (150, 150): inside the square, outside the circle → red.
    assert_eq!(img.px(25, 95), [255, 0, 0, 255]);
    // Outside both: background.
    assert_eq!(img.px(5, 5), [255, 255, 255, 255]);
}

#[test]
fn colr_foreground_uses_run_color() {
    let green = Color::from_rgba8(0, 160, 64, 255);
    let img = colr_render('D', green);
    assert_eq!(img.px(60, 60), [0, 160, 64, 255], "palette index 0xFFFF = the run color");
}

#[test]
fn box_resample_preserves_mean() {
    use render_check::golden::text::box_downscale;
    let (w, h) = (12u32, 9u32);
    let src: Vec<[f64; 4]> = (0..w * h)
        .map(|i| {
            let v = f64::from((i * 37) % 101) / 100.0;
            [v * 0.5, v * 0.25, v, v]
        })
        .collect();
    let mean = |px: &[[f64; 4]]| -> [f64; 4] {
        let n = px.len() as f64;
        std::array::from_fn(|k| px.iter().map(|p| p[k]).sum::<f64>() / n)
    };
    // s = 1/3 divides both sides: every output averages an equal-area block → same mean.
    let (out, ow, oh) = box_downscale(&src, w, h, 1.0 / 3.0);
    assert_eq!((ow, oh), (4, 3));
    let (a, b) = (mean(&src), mean(&out));
    assert!(a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-12), "{a:?} vs {b:?}");
}
