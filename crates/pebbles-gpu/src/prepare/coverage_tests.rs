//! R01 tests: every OC row has a fixture; delivered rows render without a fallback bit,
//! undelivered rows fall back loudly.

use kurbo::{Affine, BezPath, Circle, Rect, RoundedRect, Stroke};
use pebbles_render::paint::{Glyph, Painter, Scene};
use peniko::{BlendMode, Brush, Color, Compose, Extend, Fill, ImageQuality, Mix};

use super::{ALL, DELIVERED, Feature as F, Policy, policy};
use crate::prepare::test_util::prepare_scene;

/// Rows no display-list op can produce yet (their ops / data are added by the delivering card).
const NOT_PRODUCIBLE: &[(F, &str)] = &[
    (F::ImageNine, "no Op::ImageNine yet (R36)"),
    (F::DynamicImage, "no Op::DynamicImage yet (R37)"),
    (F::BlurredFill, "no Op::BlurredFill yet (R42)"),
    (F::BlurredGlyphs, "no Op::BlurredGlyphs yet (R42)"),
    (F::Vertices, "no Op::Vertices yet (R63)"),
    (F::SpriteAtlas, "no Op::Atlas yet (R64)"),
    (F::EffectBlur, "no Op::PushEffectLayer yet (L61)"),
    (F::BackdropFilter, "no Op::PushBackdropFilter yet (L62)"),
    (F::EffectColorMatrix, "no LayerEffect yet (L63)"),
    (F::EffectMorphology, "no LayerEffect yet (L64)"),
    (F::ProjectedLayer, "no Op::PushProjectedLayer yet (L65)"),
    (F::ShaderBrush, "no Op::ShaderFill yet (L43)"),
    (F::GlyphsColor, "no colour font is bundled; colour content is known only at raster (R23/R29)"),
    (F::ClipEllipse, "Geometry has no Ellipse variant: an oval clip arrives as a Path (OC-24, L01)"),
];

/// Rows whose contract is "do not crash" (or exact by contract): no bit is required.
const NOT_CRASH: &[F] = &[
    F::LayerUnbalanced,
    F::ImageMalformed,
    F::ShadowDegenerate,
    F::GlyphsDegenerate,
    F::NonFinite,
    F::Offscreen,
    F::GlyphsEvenOdd,
];

const RED: Color = Color::from_rgba8(255, 0, 0, 255);
const R: Rect = Rect::new(8.0, 8.0, 40.0, 40.0);

fn image(format: peniko::ImageFormat, alpha: peniko::ImageAlphaType, w: u32, h: u32) -> peniko::ImageBrush {
    peniko::ImageBrush::new(peniko::ImageData {
        data: peniko::Blob::from(vec![200u8; (w * h * 4) as usize]),
        format,
        alpha_type: alpha,
        width: w,
        height: h,
    })
}

fn rgba(w: u32, h: u32) -> peniko::ImageBrush {
    image(peniko::ImageFormat::Rgba8, peniko::ImageAlphaType::Alpha, w, h)
}

fn gradient() -> peniko::Gradient {
    peniko::Gradient::new_linear((0.0, 0.0), (64.0, 0.0)).with_stops([RED, Color::BLACK].as_slice())
}

fn glyphs(p: &mut Painter<'_>, size: f32, brush: &Brush, xf: Affine, glyph_xf: Option<Affine>, style: Fill) {
    let font = crate::test_support::bundled_font();
    p.draw_glyphs(
        &font,
        size,
        &[],
        brush,
        xf,
        glyph_xf,
        style,
        std::iter::once(Glyph { id: 36, x: 10.0, y: 40.0 }),
    );
}

fn layer(p: &mut Painter<'_>, blend: impl Into<BlendMode>, alpha: f32, xf: Affine, clip: &impl kurbo::Shape) {
    p.push_layer(Fill::NonZero, blend, alpha, xf, clip);
    p.fill(Fill::NonZero, Affine::IDENTITY, RED, None, &R);
    p.pop_layer();
}

fn donut() -> BezPath {
    let mut d = Circle::new((32.0, 32.0), 24.0).to_path(0.1);
    d.extend(Circle::new((32.0, 32.0), 10.0).to_path(0.1));
    d
}

use kurbo::Shape as _;

/// The smallest scene exercising row `f` on a 64×64 target, or `None` if it cannot be produced.
fn fixture(f: F) -> Option<Scene> {
    let mut s = Scene::new();
    let p = &mut Painter::new(&mut s);
    let id = Affine::IDENTITY;
    let rot = Affine::rotate(0.3).then_translate((32.0, 0.0).into());
    match f {
        F::FillSolid => p.fill(Fill::NonZero, id, RED, None, &R),
        F::FillEvenOdd => p.fill(Fill::EvenOdd, id, RED, None, &donut()),
        F::FillGradientLinear => p.fill(Fill::NonZero, id, &gradient(), None, &R),
        F::FillGradientRadial => {
            let g =
                peniko::Gradient::new_radial((32.0, 32.0), 20.0).with_stops([RED, Color::BLACK].as_slice());
            p.fill(Fill::NonZero, id, &g, None, &R);
        }
        F::FillGradientSweep => {
            let g = peniko::Gradient::new_sweep((32.0, 32.0), 0.0, std::f32::consts::TAU)
                .with_stops([RED, Color::BLACK].as_slice());
            p.fill(Fill::NonZero, id, &g, None, &R);
        }
        F::GradientStopsExtend => {
            p.fill(Fill::NonZero, id, &gradient().with_extend(Extend::Repeat), None, &R)
        }
        F::GradientNonSrgbInterpolation => {
            let g = gradient().with_interpolation_cs(peniko::color::ColorSpaceTag::Oklab);
            p.fill(Fill::NonZero, id, &g, None, &R);
        }
        F::GradientBrushTransform => p.fill(Fill::NonZero, id, &gradient(), Some(Affine::scale(2.0)), &R),
        F::FillImageBrush => p.fill(Fill::NonZero, id, &Brush::Image(rgba(2, 2)), None, &R),
        F::ImageBrushTransform => {
            p.fill(Fill::NonZero, id, &Brush::Image(rgba(2, 2)), Some(Affine::scale(2.0)), &R)
        }
        F::StrokeSolid => p.stroke(&Stroke::new(2.0), id, RED, None, &R),
        F::StrokeDashes => p.stroke(&Stroke::new(2.0).with_dashes(0.0, [4.0, 2.0]), id, RED, None, &R),
        F::StrokeJoinsCaps => {
            let s = Stroke::new(4.0).with_join(kurbo::Join::Miter).with_caps(kurbo::Cap::Butt);
            p.stroke(&s, id, RED, None, &R);
        }
        F::StrokeHairline => p.stroke(&Stroke::new(0.25), id, RED, None, &R),
        F::StrokeGradient => p.stroke(&Stroke::new(2.0), id, &gradient(), None, &R),
        F::StrokeImageBrush => p.stroke(&Stroke::new(2.0), id, &Brush::Image(rgba(2, 2)), None, &R),
        F::ClipRect => layer(p, Mix::Normal, 1.0, id, &Rect::new(0.0, 0.0, 32.0, 32.0)),
        F::ClipRectQuarterTurn => {
            layer(
                p,
                Mix::Normal,
                1.0,
                Affine::rotate(std::f64::consts::FRAC_PI_2).then_translate((64.0, 0.0).into()),
                &R,
            );
        }
        F::ClipRectRotated => layer(p, Mix::Normal, 1.0, rot, &R),
        F::ClipRRectZeroRadius => layer(p, Mix::Normal, 1.0, id, &RoundedRect::from_rect(R, 0.0)),
        F::ClipRRect => layer(p, Mix::Normal, 1.0, id, &RoundedRect::from_rect(R, 6.0)),
        F::ClipCircle => layer(p, Mix::Normal, 1.0, id, &Circle::new((32.0, 32.0), 20.0)),
        F::ClipPath => layer(p, Mix::Normal, 1.0, id, &donut()),
        F::ClipEvenOdd => {
            p.push_layer(Fill::EvenOdd, Mix::Normal, 1.0, id, &donut());
            p.fill(Fill::NonZero, id, RED, None, &R);
            p.pop_layer();
        }
        F::ClipNested => {
            p.push_layer(Fill::NonZero, Mix::Normal, 1.0, id, &Rect::new(0.0, 0.0, 48.0, 48.0));
            layer(p, Mix::Normal, 1.0, id, &Rect::new(16.0, 16.0, 64.0, 64.0));
            p.pop_layer();
        }
        F::LayerAlpha => layer(p, Mix::Normal, 0.5, id, &Rect::new(0.0, 0.0, 64.0, 64.0)),
        F::LayerMix => layer(p, Mix::Multiply, 1.0, id, &Rect::new(0.0, 0.0, 64.0, 64.0)),
        F::LayerCompose => {
            layer(p, BlendMode::new(Mix::Normal, Compose::DestIn), 1.0, id, &Rect::new(0.0, 0.0, 64.0, 64.0))
        }
        F::LayerMixAndCompose => {
            layer(p, BlendMode::new(Mix::Multiply, Compose::Xor), 1.0, id, &Rect::new(0.0, 0.0, 64.0, 64.0));
        }
        F::LayerUnbalanced => {
            p.pop_layer();
            p.fill(Fill::NonZero, id, RED, None, &R);
        }
        F::ImageRgba8 => p.draw_image(&rgba(4, 4), Affine::translate((8.0, 8.0))),
        F::ImageBgra8 => p.draw_image(
            &image(peniko::ImageFormat::Bgra8, peniko::ImageAlphaType::Alpha, 4, 4),
            Affine::translate((8.0, 8.0)),
        ),
        F::ImagePremultiplied => p.draw_image(
            &image(peniko::ImageFormat::Rgba8, peniko::ImageAlphaType::AlphaPremultiplied, 4, 4),
            Affine::translate((8.0, 8.0)),
        ),
        F::ImageQuality => p.draw_image(&rgba(4, 4).with_quality(ImageQuality::High), Affine::scale(4.0)),
        F::ImageExtend => {
            p.draw_image(&rgba(4, 4).with_extend(Extend::Repeat), Affine::translate((8.0, 8.0)))
        }
        F::ImageAlpha => p.draw_image(&rgba(4, 4).with_alpha(0.5), Affine::translate((8.0, 8.0))),
        F::ImageTransform => p.draw_image(&rgba(4, 4), rot),
        F::ImageOversize => p.draw_image(&rgba(64, 2), Affine::scale(0.5)),
        F::ImageMalformed => p.draw_image(&rgba(0, 4), id),
        F::ImageMipmaps => p.draw_image(&rgba(64, 64), Affine::scale(0.25)),
        F::ShadowAxisAligned => p.draw_blurred_rounded_rect(id, R, Color::BLACK, 4.0, 4.0),
        F::ShadowRotated => p.draw_blurred_rounded_rect(rot, R, Color::BLACK, 4.0, 4.0),
        F::ShadowDegenerate => p.draw_blurred_rounded_rect(id, R, Color::BLACK, -1.0, f64::NAN),
        F::GlyphsSolid => glyphs(p, 16.0, &Brush::Solid(RED), id, None, Fill::NonZero),
        F::GlyphsVariable => {
            let font = crate::test_support::bundled_font();
            let g = std::iter::once(Glyph { id: 36, x: 10.0, y: 40.0 });
            p.draw_glyphs(&font, 16.0, &[0, 8848], &Brush::Solid(RED), id, None, Fill::NonZero, g);
        }
        F::GlyphsTransform => {
            glyphs(p, 16.0, &Brush::Solid(RED), id, Some(Affine::skew(-0.2, 0.0)), Fill::NonZero)
        }
        F::GlyphsRotated => glyphs(p, 16.0, &Brush::Solid(RED), rot, None, Fill::NonZero),
        F::GlyphsLarge => glyphs(p, 300.0, &Brush::Solid(RED), id, None, Fill::NonZero),
        F::GlyphsNonSolidBrush => glyphs(p, 16.0, &Brush::Gradient(gradient()), id, None, Fill::NonZero),
        F::GlyphsEvenOdd => glyphs(p, 16.0, &Brush::Solid(RED), id, None, Fill::EvenOdd),
        F::GlyphsDegenerate => glyphs(p, f32::NAN, &Brush::Solid(RED), id, None, Fill::NonZero),
        F::GlyphsColorRotated => {
            // R29: the committed COLR test font's glyph `A`, rotated.
            use crate::raster::color_glyph::tests::{colr_font, gid};
            let font = colr_font();
            let g = Glyph { id: u32::from(gid(&font, 'A')), x: 8.0, y: 40.0 };
            let brush = Brush::Solid(RED);
            p.draw_glyphs(&font, 32.0, &[], &brush, rot, None, Fill::NonZero, std::iter::once(g));
        }
        F::FragmentAppend => {
            let mut inner = Scene::new();
            Painter::new(&mut inner).fill(Fill::NonZero, id, RED, None, &R);
            p.append(&inner, Some(rot));
        }
        F::NonFinite => p.fill(Fill::NonZero, id, RED, None, &Rect::new(f64::NAN, 0.0, 8.0, 8.0)),
        F::Offscreen => p.fill(Fill::NonZero, id, RED, None, &Rect::new(500.0, 500.0, 600.0, 600.0)),
        F::GradientDither => p.fill(Fill::NonZero, id, &gradient(), None, &R),
        F::ClipGpuMask => layer(p, Mix::Normal, 1.0, id, &Circle::new((32.0, 32.0), 200.0).to_path(0.1)),
        _ => return None,
    }
    Some(s)
}

#[test]
fn every_row_has_a_fixture_or_a_reason() {
    for f in ALL {
        let listed = NOT_PRODUCIBLE.iter().any(|(g, _)| *g == f);
        assert_eq!(fixture(f).is_some(), !listed, "{f:?}: fixture/NOT_PRODUCIBLE mismatch");
    }
}

#[test]
fn delivered_features_render_without_fallback() {
    for &f in DELIVERED {
        let scene = fixture(f).unwrap_or_else(|| panic!("{f:?} delivered but has no fixture"));
        let (plan, _) = prepare_scene(&scene, 64, 64);
        // "Do not crash" rows are delivered by skipping (and recording) their op (R02 step 3).
        if NOT_CRASH.contains(&f) {
            continue;
        }
        assert!(!plan.fallback_features.contains(f), "{f:?} delivered but fell back");
        assert!(!plan.cmds.is_empty(), "{f:?} produced no draw");
    }
}

#[test]
fn undelivered_features_fall_back_loudly() {
    let mut silent = Vec::new();
    for f in ALL {
        if DELIVERED.contains(&f) || NOT_CRASH.contains(&f) {
            if let Some(scene) = fixture(f) {
                prepare_scene(&scene, 64, 64); // must not panic
            }
            continue;
        }
        let Some(scene) = fixture(f) else { continue };
        let (plan, _) = prepare_scene(&scene, 64, 64);
        if !plan.fallback_features.contains(f) {
            silent.push(f);
        }
    }
    assert!(silent.is_empty(), "not delivered but drew without a fallback bit: {silent:?}");
}

#[test]
fn every_row_in_oc_order_and_may_rows() {
    assert_eq!(ALL.len(), 71);
    for (i, f) in ALL.iter().enumerate() {
        assert_eq!(*f as usize, i, "{f:?} out of OC order");
    }
    let may: Vec<F> = ALL.into_iter().filter(|f| policy(*f) == Policy::May).collect();
    assert_eq!(
        may,
        [
            F::GradientNonSrgbInterpolation,
            F::FillImageBrush,
            F::ImageBrushTransform,
            F::StrokeImageBrush,
            F::LayerMixAndCompose,
            F::GlyphsEvenOdd
        ]
    );
}

#[test]
#[ignore = "Phase 5 gate (05 §12): un-ignore when Phase 4 is complete"]
fn all_must_features_delivered() {
    // GPU lanes run `-- --ignored`; this gate must only assert when the Phase 5 gate asks for it.
    if !std::env::var("PEBBLES_PHASE5_GATE").is_ok_and(|v| v == "1") {
        return;
    }
    for f in ALL {
        if policy(f) == Policy::Must {
            assert!(DELIVERED.contains(&f), "{f:?} is MUST but not delivered");
        }
    }
}
