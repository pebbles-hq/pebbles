//! Image brushes (09 §6, L41): the device → image-pixel mapping of a fill or stroke painted
//! with an image under its paint transform `P` (`outer × op.transform × brush_transform`).

use kurbo::Affine;

use super::num::narrow_f32;
use crate::gpu_types::ImageBrushParams;

/// `D = P⁻¹` maps a device pixel center to image pixels; `size` is the LOGICAL image size, so
/// a downscaled upload (C13) still maps correctly. `paint_xf` must be invertible.
pub(crate) fn params(paint_xf: Affine, w: u32, h: u32) -> ImageBrushParams {
    let [a, b, c, d, e, f] = paint_xf.inverse().as_coeffs();
    let (w, h) = (w as f32, h as f32);
    ImageBrushParams {
        m0: [narrow_f32(a), narrow_f32(c), narrow_f32(e), 0.0],
        m1: [narrow_f32(b), narrow_f32(d), narrow_f32(f), 0.0],
        size: [w, h, 1.0 / w, 1.0 / h],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prepare::plan::{BrushCover, BrushPaint, DrawCmd};
    use crate::prepare::test_util::prepare_scene;
    use pebbles_render::paint::{Glyph, Painter, Scene};
    use peniko::{Brush, Fill};

    fn device_to_image(p: &ImageBrushParams, x: f32, y: f32) -> (f32, f32) {
        (p.m0[0] * x + p.m0[1] * y + p.m0[2], p.m1[0] * x + p.m1[1] * y + p.m1[2])
    }

    #[test]
    fn device_to_image_inverts_paint() {
        let p = params(Affine::scale(16.0), 2, 2);
        assert_eq!(device_to_image(&p, 16.0, 16.0), (1.0, 1.0));
        assert_eq!(p.size, [2.0, 2.0, 0.5, 0.5]);
    }

    fn brush(alpha: f32) -> peniko::ImageBrush {
        let mut b = crate::test_support::rgba_image(2, 2, [255; 4]);
        b.sampler.alpha = alpha;
        b
    }

    #[test]
    fn alpha_combines_sampler_and_fold() {
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        p.push_layer(
            Fill::NonZero,
            peniko::Mix::Normal,
            0.5,
            Affine::IDENTITY,
            &kurbo::Rect::new(0.0, 0.0, 64.0, 64.0),
        );
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &brush(0.5),
            Some(Affine::scale(16.0)),
            &kurbo::Rect::new(0.0, 0.0, 32.0, 32.0),
        );
        p.pop_layer();
        let (plan, _) = prepare_scene(&s, 64, 64);
        assert!(matches!(
            plan.cmds[..],
            [DrawCmd::Brush { paint: BrushPaint::Image(0), cover: BrushCover::Shape(_), .. }]
        ));
        assert_eq!(plan.shapes[0].color, [0.25; 4], "sampler alpha 0.5 × fold 0.5");
    }

    #[test]
    fn singular_transform_skips() {
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &brush(1.0),
            Some(Affine::scale(0.0)),
            &kurbo::Rect::new(0.0, 0.0, 32.0, 32.0),
        );
        let (plan, _) = prepare_scene(&s, 64, 64);
        assert!(plan.cmds.is_empty());
        assert_eq!(plan.stats.image_brush_skipped, 1);
    }

    #[test]
    fn image_brush_on_glyphs_uses_outlines() {
        let font = crate::test_support::bundled_font();
        let font_ref = skrifa::FontRef::from_index(font.data.data(), 0).expect("font");
        let gid = skrifa::MetadataProvider::charmap(&font_ref).map('H').expect("H").to_u32();
        let mut s = Scene::new();
        Painter::new(&mut s).draw_glyphs(
            &font,
            24.0,
            &[],
            &Brush::Image(brush(1.0)),
            Affine::IDENTITY,
            None,
            Fill::NonZero,
            [Glyph { id: gid, x: 10.0, y: 30.0 }].into_iter(),
        );
        let (plan, _) = prepare_scene(&s, 64, 64);
        assert!(matches!(
            plan.cmds[..],
            [DrawCmd::Brush { paint: BrushPaint::Image(0), cover: BrushCover::Mask(_), .. }]
        ));
    }
}
