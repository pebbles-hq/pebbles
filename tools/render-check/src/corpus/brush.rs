//! `brush/` image brushes on fills and strokes (05 §7) — Slice P4, 128×128, judged by the
//! golden model and analytic probes (the transitional smoke reference skips image brushes).

use pebbles_render::paint::{ShaderBrush, ShaderProgram, kurbo, peniko};

use kurbo::{Affine, Rect, RoundedRect, Stroke};
use peniko::{Extend, Fill, ImageAlphaType, ImageBrush, ImageData, ImageFormat, ImageQuality};

use crate::case::{Case, Category::Gradient, Category::Image, Slice::P4, ops};
use crate::probe::{self, noise_rgba8};

pub fn cases() -> Vec<Case> {
    vec![
        ops("brush/image_fill_repeat", Image, P4, 128, 128, |p| {
            let b = noise().with_extend(Extend::Repeat);
            p.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                &b,
                Some(Affine::scale(2.)),
                &Rect::new(8., 8., 120., 120.),
            );
        })
        .probe("image_fill_texels", probe::image_fill_texels),
        ops("brush/image_fill_rrect_reflect", Image, P4, 128, 128, |p| {
            let b = noise().with_extend(Extend::Reflect);
            p.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                &b,
                Some(Affine::scale(2.)),
                &RoundedRect::new(8., 8., 120., 120., 24.),
            );
        }),
        ops("brush/image_stroke", Image, P4, 128, 128, |p| {
            let b = noise().with_extend(Extend::Repeat);
            p.stroke(
                &Stroke::new(12.),
                Affine::IDENTITY,
                &b,
                Some(Affine::scale(2.)),
                &kurbo::Circle::new((64., 64.), 44.),
            );
        }),
        ops("brush/shader_uv_rect", Gradient, P4, 128, 128, |p| {
            p.fill_shader(Fill::NonZero, Affine::IDENTITY, &shader(UV, &[]), &Rect::new(8., 8., 120., 120.));
        }),
        ops("brush/shader_star_uniforms", Gradient, P4, 128, 128, |p| {
            let b = shader(TINT, &[0.85, 0.35, 0.15, 0.9]);
            p.fill_shader(Fill::NonZero, Affine::translate((4., 2.)), &b, &star());
        }),
        ops("brush/shader_rings_stroke", Gradient, P4, 128, 128, |p| {
            let b = shader(RINGS, &[]);
            p.stroke_shader(&Stroke::new(10.), Affine::IDENTITY, &b, &kurbo::Circle::new((64., 64.), 40.));
        }),
    ]
}

/// The 16×16 `noise_rgba8(.., 7)` image, `Low` quality (texel-exact under `scale(2)`).
fn noise() -> ImageBrush {
    ImageBrush::new(ImageData {
        data: peniko::Blob::from(noise_rgba8(16, 16, 7)),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: 16,
        height: 16,
    })
    .with_quality(ImageQuality::Low)
}

/// `local.x / size.x` in red, `local.y / size.y` in green (opaque).
const UV: &str = "fn pebbles_paint(local: vec2<f32>, device: vec2<f32>) -> vec4<f32> {
    let t = local / u.size;
    return vec4<f32>(t.x, t.y, 0.2, 1.0);
}";

/// `u.values[0]` premultiplied by its alpha, fading with `local.y`.
const TINT: &str = "fn pebbles_paint(local: vec2<f32>, device: vec2<f32>) -> vec4<f32> {
    let c = u.values[0];
    let a = c.a * (1.0 - 0.5 * local.y / u.size.y);
    return vec4<f32>(c.rgb * a, a);
}";

/// Rings around the shape's center (opaque).
const RINGS: &str = "fn pebbles_paint(local: vec2<f32>, device: vec2<f32>) -> vec4<f32> {
    let d = length(local - u.size * 0.5);
    let t = 0.5 + 0.5 * sin(d * 0.4);
    return vec4<f32>(0.8 * t, 0.3, 1.0 - t, 1.0);
}";

fn shader(src: &'static str, values: &[f32]) -> ShaderBrush {
    thread_local! {
        static PROGRAMS: std::cell::RefCell<Vec<(&'static str, ShaderProgram)>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    // One program per source, so every frame reuses its compiled pipelines.
    let program = PROGRAMS.with(|ps| {
        let mut ps = ps.borrow_mut();
        if let Some((_, p)) = ps.iter().find(|(s, _)| *s == src) {
            return p.clone();
        }
        let p = ShaderProgram::new(src);
        ps.push((src, p.clone()));
        p
    });
    let none: [ImageBrush; 0] = [];
    ShaderBrush::new(program, values.to_vec(), none, Affine::IDENTITY).expect("corpus brush")
}

fn star() -> kurbo::BezPath {
    let mut p = kurbo::BezPath::new();
    for i in 0..10u8 {
        let r = if i % 2 == 0 { 56.0 } else { 24.0 };
        let a = std::f64::consts::PI * f64::from(i) / 5.0 - std::f64::consts::FRAC_PI_2;
        let pt = (60.0 + r * a.cos(), 62.0 + r * a.sin());
        if i == 0 {
            p.move_to(pt);
        } else {
            p.line_to(pt);
        }
    }
    p.close_path();
    p
}

/// The CPU mirror of each corpus program (golden model, 09 §6.1).
pub fn shader_oracle(src: &str) -> Option<crate::golden::paint::ShaderOracle> {
    fn uv(l: kurbo::Point, _: kurbo::Point, _: &[f32], s: (f64, f64)) -> [f64; 4] {
        [l.x / s.0, l.y / s.1, 0.2, 1.0]
    }
    fn tint(l: kurbo::Point, _: kurbo::Point, v: &[f32], s: (f64, f64)) -> [f64; 4] {
        let c = |i: usize| f64::from(v.get(i).copied().unwrap_or(0.0));
        let a = c(3) * (1.0 - 0.5 * l.y / s.1);
        [c(0) * a, c(1) * a, c(2) * a, a]
    }
    fn rings(l: kurbo::Point, _: kurbo::Point, _: &[f32], s: (f64, f64)) -> [f64; 4] {
        let d = (l - kurbo::Point::new(s.0 * 0.5, s.1 * 0.5)).hypot();
        let t = 0.5 + 0.5 * (d * 0.4).sin();
        [0.8 * t, 0.3, 1.0 - t, 1.0]
    }
    match src {
        UV => Some(uv),
        TINT => Some(tint),
        RINGS => Some(rings),
        _ => None,
    }
}
