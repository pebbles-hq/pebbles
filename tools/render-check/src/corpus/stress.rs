//! stress/ — Slice Robustness, 1920×1080, Category Stress (§7.2, V26). Seeded `XorShift64`;
//! judged by self-goldens (V12), UNJUDGED until then. Slowness is a defect, never a timeout.

use pebbles_core::IntoWidget as _;
use pebbles_render::paint::{Painter, kurbo, peniko};
use pebbles_testing::Harness;

use kurbo::{Affine, BezPath, Rect, RoundedRect, Stroke};
use peniko::{Color, Fill, Mix};

use crate::case::{Case, Category::Stress, Slice::Robustness, harness, ops};
use crate::rng::XorShift64;

const W: u32 = 1920;
const H: u32 = 1080;

pub fn cases() -> Vec<Case> {
    vec![
        ops("stress/rects_10k", Stress, Robustness, W, H, rects_10k),
        ops("stress/rrects_strokes_5k", Stress, Robustness, W, H, rrects_strokes_5k),
        harness("stress/glyph_runs_1k", Stress, Robustness, W, H, glyph_runs_1k).no_refs(),
        ops("stress/clips_8deep_x100", Stress, Robustness, W, H, clips_8deep_x100),
        ops("stress/paths_200x500seg", Stress, Robustness, W, H, paths_200x500seg).no_refs(),
        ops("stress/huge_path_100k_segments", Stress, Robustness, W, H, huge_path_100k_segments),
        ops("stress/image_4k_fit", Stress, Robustness, W, H, image_4k_fit).no_refs(),
        ops("stress/hairline_grid_dense", Stress, Robustness, W, H, hairline_grid_dense),
    ]
}

fn color(rng: &mut XorShift64) -> Color {
    let v = rng.next_u64();
    Color::from_rgba8(v as u8, (v >> 8) as u8, (v >> 16) as u8, 255)
}

fn rects_10k(p: &mut Painter<'_>) {
    let mut rng = XorShift64::new(1);
    for _ in 0..10_000 {
        let (x, y) = (rng.range_f64(0.0, 1900.0), rng.range_f64(0.0, 1060.0));
        let (w, h) = (rng.range_f64(4.0, 24.0), rng.range_f64(4.0, 24.0));
        let c = color(&mut rng);
        p.fill(Fill::NonZero, Affine::IDENTITY, c, None, &Rect::new(x, y, x + w, y + h));
    }
}

fn rrects_strokes_5k(p: &mut Painter<'_>) {
    let mut rng = XorShift64::new(2);
    for _ in 0..5_000 {
        let (x, y) = (rng.range_f64(0.0, 1880.0), rng.range_f64(0.0, 1040.0));
        let (w, h) = (rng.range_f64(8.0, 40.0), rng.range_f64(8.0, 40.0));
        let style = Stroke::new(rng.range_f64(1.0, 3.0));
        let c = color(&mut rng);
        p.stroke(&style, Affine::IDENTITY, c, None, &RoundedRect::new(x, y, x + w, y + h, 4.0));
    }
}

fn glyph_runs_1k(h: &mut Harness) {
    let words = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta"];
    let children: Vec<_> = (0..1000)
        .map(|i| {
            let line: Vec<&str> = words.iter().cycle().skip(i % 8).take(8).copied().collect();
            pebbles_widgets::text(line.join(" ")).into_widget()
        })
        .collect();
    h.mount_widget(pebbles_widgets::wrap(children).spacing(4.0));
}

fn clips_8deep_x100(p: &mut Painter<'_>) {
    let mut rng = XorShift64::new(4);
    for g in 0..100 {
        let (x, y) = (f64::from(g % 20) * 96.0, f64::from(g / 20) * 216.0);
        for d in 0..8 {
            let inset = f64::from(d) * 5.0;
            let r = Rect::new(x + inset, y + inset, x + 96.0 - inset, y + 216.0 - inset);
            if d % 2 == 0 {
                p.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &r);
            } else {
                p.push_layer(
                    Fill::NonZero,
                    Mix::Normal,
                    1.0,
                    Affine::IDENTITY,
                    &RoundedRect::from_rect(r, 8.0),
                );
            }
        }
        let c = color(&mut rng);
        p.fill(Fill::NonZero, Affine::IDENTITY, c, None, &Rect::new(x, y, x + 96.0, y + 216.0));
        for _ in 0..8 {
            p.pop_layer();
        }
    }
}

fn paths_200x500seg(p: &mut Painter<'_>) {
    let mut rng = XorShift64::new(5);
    for _ in 0..200 {
        let mut path = BezPath::new();
        let (cx, cy) = (rng.range_f64(100.0, 1820.0), rng.range_f64(100.0, 980.0));
        path.move_to((cx, cy));
        for _ in 0..500 {
            path.line_to((cx + rng.range_f64(-100.0, 100.0), cy + rng.range_f64(-100.0, 100.0)));
        }
        path.close_path();
        let c = color(&mut rng).with_alpha(0.3);
        p.fill(Fill::EvenOdd, Affine::IDENTITY, c, None, &path);
    }
}

fn huge_path_100k_segments(p: &mut Painter<'_>) {
    let mut path = BezPath::new();
    let n = 100_000;
    for i in 0..n {
        let t = std::f64::consts::TAU * f64::from(i) / f64::from(n);
        let r = 400.0 + 60.0 * (t * 37.0).sin();
        let pt = (960.0 + r * t.cos(), 540.0 + r * t.sin());
        if i == 0 {
            path.move_to(pt);
        } else {
            path.line_to(pt);
        }
    }
    path.close_path();
    p.fill(Fill::NonZero, Affine::IDENTITY, Color::from_rgb8(59, 130, 246), None, &path);
}

fn image_4k_fit(p: &mut Painter<'_>) {
    let (w, h) = (3840u32, 2160u32);
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            px.extend_from_slice(&[(x / 15) as u8, (y / 9) as u8, ((x ^ y) & 0xFF) as u8, 255]);
        }
    }
    let image = peniko::ImageData {
        data: peniko::Blob::from(px),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::Alpha,
        width: w,
        height: h,
    };
    p.draw_image(&peniko::ImageBrush::new(image), Affine::scale(0.5));
}

fn hairline_grid_dense(p: &mut Painter<'_>) {
    let ink = Color::from_rgb8(31, 41, 55);
    let style = Stroke::new(1.0);
    for i in 0..(W / 4) {
        let x = f64::from(i * 4) + 0.5;
        p.stroke(&style, Affine::IDENTITY, ink, None, &kurbo::Line::new((x, 0.0), (x, f64::from(H))));
    }
    for i in 0..(H / 4) {
        let y = f64::from(i * 4) + 0.5;
        p.stroke(&style, Affine::IDENTITY, ink, None, &kurbo::Line::new((0.0, y), (f64::from(W), y)));
    }
}
