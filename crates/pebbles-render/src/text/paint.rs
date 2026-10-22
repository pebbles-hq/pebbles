//! Glyph runs from a [`TextLayout`] (03 §3.9): backend-neutral `draw_glyphs` ops, one per run
//! of glyphs sharing face, weight, size, synthesis flags and brush slot. Text never
//! rasterizes; the renderer owns the atlas (DR-07).

use kurbo::{Affine, Rect};
use pebbles_foundation::Offset;
use peniko::{Brush, Fill};

use super::TextEnv;
use super::layout::TextLayout;
use crate::paint::{Glyph, Painter};
use crate::tree::PaintCx;

/// The faux-italic shear (cosmic-text sets FAKE_ITALIC when the face is upright).
fn fake_italic() -> Affine {
    Affine::skew(14f64.to_radians().tan(), 0.0)
}

/// Paint `layout`'s glyph runs at `origin` with `brushes` (slot 0 = base color).
#[allow(dead_code)] // RenderParagraph paints through it from T22.
pub(crate) fn paint_text_layout(
    cx: &mut PaintCx<'_>,
    layout: &TextLayout,
    origin: Offset,
    brushes: &[Brush],
) {
    let visible = cx.visible();
    paint_into(&mut cx.scene, cx.text, visible, layout, origin, brushes);
}

/// [`paint_text_layout`] on an explicit painter / text env / visible rect.
pub(crate) fn paint_into(
    scene: &mut Painter<'_>,
    text: &mut TextEnv,
    visible: Rect,
    layout: &TextLayout,
    origin: Offset,
    brushes: &[Brush],
) {
    let transform = Affine::translate((origin.x, origin.y));
    for line in &layout.lines {
        let top = origin.y + f64::from(line.top);
        if top + f64::from(line.height) < visible.y0 {
            continue;
        }
        if top > visible.y1 {
            break;
        }
        if !line.from_elided && layout.shaped.paras[line.para].placeholder {
            continue;
        }
        let glyphs = &line.layout.glyphs;
        let mut start = 0;
        while start < glyphs.len() {
            let g0 = &glyphs[start];
            let key = (
                g0.font_id,
                g0.font_weight.0,
                g0.font_size.to_bits(),
                g0.cache_key_flags.bits(),
                g0.metadata,
            );
            let end = glyphs[start..]
                .iter()
                .position(|g| {
                    (g.font_id, g.font_weight.0, g.font_size.to_bits(), g.cache_key_flags.bits(), g.metadata)
                        != key
                })
                .map_or(glyphs.len(), |n| start + n);
            let run = &glyphs[start..end];
            start = end;
            if g0.font_size <= 0.0 {
                continue;
            }
            let pen = |g: &cosmic_text::LayoutGlyph| {
                (g.x + g.font_size * g.x_offset, line.baseline + g.y - g.font_size * g.y_offset)
            };
            let x0 = run.iter().map(|g| pen(g).0).fold(f32::INFINITY, f32::min);
            let x1 = run.iter().map(|g| pen(g).0 + g.w).fold(f32::NEG_INFINITY, f32::max);
            if origin.x + f64::from(x1) < visible.x0 || origin.x + f64::from(x0) > visible.x1 {
                continue;
            }
            let Some(face) = text.face(g0.font_id, g0.font_weight.0) else { continue };
            let brush = brushes.get(g0.metadata).or(brushes.first()).cloned().unwrap_or_default();
            let glyph_transform =
                g0.cache_key_flags.contains(cosmic_text::CacheKeyFlags::FAKE_ITALIC).then(fake_italic);
            scene.draw_glyphs(
                &face.font,
                g0.font_size,
                &face.coords,
                &brush,
                transform,
                glyph_transform,
                Fill::NonZero,
                run.iter().map(|g| {
                    let (x, y) = pen(g);
                    Glyph { id: u32::from(g.glyph_id), x, y }
                }),
            );
            crate::stats::bump_glyph_run();
        }
        paint_decorations(scene, visible, line, origin, brushes);
    }
}

/// Underline / strikethrough rects of one line (§3.9.5, Parley formulas; never rounded).
fn paint_decorations(
    scene: &mut Painter<'_>,
    visible: Rect,
    line: &super::layout::VisualLine,
    origin: Offset,
    brushes: &[Brush],
) {
    let glyphs = &line.layout.glyphs;
    for span in &line.layout.decorations {
        let run = glyphs.get(span.glyph_range.clone()).unwrap_or(&[]);
        let Some(first) = run.first() else { continue };
        let x0 = run.iter().map(|g| g.x).fold(f32::INFINITY, f32::min);
        let x1 = run.iter().map(|g| g.x + g.w).fold(f32::NEG_INFINITY, f32::max);
        let (ox, oy) = (origin.x, origin.y);
        if ox + f64::from(x1) < visible.x0 || ox + f64::from(x0) > visible.x1 {
            continue;
        }
        let brush = brushes.get(first.metadata).or(brushes.first()).cloned().unwrap_or_default();
        let size = span.font_size;
        let deco = &span.data.text_decoration;
        let mut rect = |metrics: &cosmic_text::DecorationMetrics| {
            let top = line.baseline - metrics.offset * size;
            let r = Rect::new(
                f64::from(x0),
                f64::from(top),
                f64::from(x1),
                f64::from(top + metrics.thickness * size),
            );
            scene.fill(Fill::NonZero, Affine::IDENTITY, &brush, None, &(r + kurbo::Vec2::new(ox, oy)));
        };
        if deco.underline != cosmic_text::UnderlineStyle::None {
            rect(&span.data.underline_metrics);
        }
        if deco.strikethrough {
            rect(&span.data.strikethrough_metrics);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::{ParagraphStyle, TextSpanStyle};
    use crate::paint::Scene;
    use crate::paint::display_list::Op;
    use crate::text::layout_for_test;
    use peniko::{Color, FontData};

    type GlyphOp = (FontData, f32, Vec<i16>, Option<Affine>, Affine, Brush, Vec<Glyph>);

    fn glyph_ops(scene: &Scene) -> Vec<GlyphOp> {
        scene
            .ops()
            .iter()
            .filter_map(|op| match op {
                Op::Glyphs {
                    font,
                    font_size,
                    normalized_coords,
                    brush,
                    transform,
                    glyph_transform,
                    glyphs,
                    ..
                } => Some((
                    font.clone(),
                    *font_size,
                    normalized_coords.clone(),
                    *glyph_transform,
                    *transform,
                    brush.clone(),
                    glyphs.clone(),
                )),
                _ => None,
            })
            .collect()
    }

    fn style(family: &str, size: f32) -> ParagraphStyle {
        ParagraphStyle { font_family: Some(family.into()), font_size: size, ..ParagraphStyle::default() }
    }

    fn brushes() -> Vec<Brush> {
        vec![
            Brush::Solid(Color::BLACK),
            Brush::Solid(Color::WHITE),
            Brush::Solid(pebbles_foundation::palette::RED),
        ]
    }

    fn paint(
        env: &mut TextEnv,
        text: &str,
        s: &ParagraphStyle,
        spans: &[TextSpanStyle],
        visible: Rect,
    ) -> Vec<GlyphOp> {
        let layout = layout_for_test(env, text, s, spans, None);
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        paint_into(&mut p, env, visible, &layout, Offset::new(10.0, 20.0), &brushes());
        glyph_ops(&scene)
    }

    const ALL: Rect = Rect::new(-1e9, -1e9, 1e9, 1e9);

    #[test]
    fn mono_abc_one_run() {
        let ops = paint(&mut TextEnv::new(), "abc", &style("JetBrains Mono", 20.0), &[], ALL);
        assert_eq!(ops.len(), 1);
        let (_, size, coords, gt, tr, _, glyphs) = &ops[0];
        assert_eq!(glyphs.len(), 3);
        for (g, x) in glyphs.iter().zip([0.0, 12.0, 24.0]) {
            assert!((g.x - x).abs() <= 0.01 && (g.y - 19.2).abs() <= 0.01, "{g:?}");
        }
        assert_eq!(*size, 20.0);
        assert!(coords.is_empty());
        assert_eq!(*tr, Affine::translate((10.0, 20.0)));
        assert_eq!(*gt, None);
    }

    #[test]
    fn weight_change_splits_runs() {
        let mut bold = TextSpanStyle::new(1..2);
        bold.weight = Some(700.0);
        let ops = paint(&mut TextEnv::new(), "ab", &style("Inter", 16.0), &[bold], ALL);
        assert_eq!(ops.len(), 2);
        assert!(!ops[1].2.is_empty());
    }

    #[test]
    #[cfg(debug_assertions)] // shape counter is debug-only
    fn color_change_splits_runs_not_shape() {
        let mut env = TextEnv::new();
        let spans = |c| {
            let mut s = TextSpanStyle::new(1..2);
            s.color = Some(c);
            vec![TextSpanStyle::new(0..0), s]
        };
        let before = crate::text::shape_count();
        let ops = paint(&mut env, "ab", &style("Inter", 16.0), &spans(Color::WHITE), ALL);
        paint(&mut env, "ab", &style("Inter", 16.0), &spans(pebbles_foundation::palette::BLUE), ALL);
        assert_eq!(crate::text::shape_count() - before, 1, "color values never reshape");
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0].5, brushes()[0]);
        assert_eq!(ops[1].5, brushes()[2]);
    }

    #[test]
    fn fake_italic_skews() {
        let mut env = TextEnv::new();
        let ops =
            paint(&mut env, "x", &ParagraphStyle { italic: true, ..style("Space Grotesk", 16.0) }, &[], ALL);
        assert_eq!(ops[0].3, Some(fake_italic()));
        let ops = paint(&mut env, "x", &ParagraphStyle { italic: true, ..style("Inter", 16.0) }, &[], ALL);
        assert_eq!(ops[0].3, None);
    }

    #[test]
    fn culls_lines() {
        let text: String = (0..100).map(|i| format!("line{i}\n")).collect();
        let s = style("JetBrains Mono", 20.0);
        // Lines are 24 px; origin y 20 → line 10 spans 260..284, line 12 spans 308..332.
        let visible = Rect::new(-1e9, 261.0, 1e9, 331.0);
        let ops = paint(&mut TextEnv::new(), text.trim_end(), &s, &[], visible);
        assert_eq!(ops.len(), 3, "{}", ops.len());
    }

    #[test]
    fn zero_size_emits_nothing() {
        assert!(paint(&mut TextEnv::new(), "abc", &style("Inter", 0.0), &[], ALL).is_empty());
    }

    #[test]
    fn face_blob_is_registry_blob() {
        let mut env = TextEnv::new();
        let layout = layout_for_test(&mut env, "a", &style("Inter", 16.0), &[], None);
        let g = &layout.lines[0].layout.glyphs[0];
        let id = env.face(g.font_id, g.font_weight.0).expect("face").font.data.id();
        let ops = paint(&mut env, "a", &style("Inter", 16.0), &[], ALL);
        assert_eq!(ops[0].0.data.id(), id);
    }

    fn fills(scene: &Scene) -> Vec<Rect> {
        scene
            .ops()
            .iter()
            .filter_map(|op| match op {
                Op::Fill { shape, .. } => Some(shape.bounding_box()),
                _ => None,
            })
            .collect()
    }

    fn deco(text: &str, s: &ParagraphStyle, spans: &[TextSpanStyle]) -> (Vec<Rect>, f32) {
        let mut env = TextEnv::new();
        let layout = layout_for_test(&mut env, text, s, spans, None);
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        paint_into(&mut p, &mut env, ALL, &layout, Offset::new(0.0, 0.0), &brushes());
        (fills(&scene), layout.width())
    }

    fn near(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn underline_inter16() {
        let (rects, w) = deco("Hi", &ParagraphStyle { underline: true, ..style("Inter", 16.0) }, &[]);
        assert_eq!(rects.len(), 1);
        let r = rects[0];
        assert!(near(r.y0, 15.4203125 + 2.71875, 0.01), "{r:?}");
        assert!(near(r.height(), 1.09375, 0.001), "{r:?}");
        assert!(near(r.x0, 0.0, 0.01) && near(r.x1, f64::from(w), 0.01), "{r:?} {w}");
    }

    #[test]
    fn strikethrough_mono20() {
        let (rects, _) =
            deco("abc", &ParagraphStyle { strikethrough: true, ..style("JetBrains Mono", 20.0) }, &[]);
        assert_eq!(rects.len(), 1);
        let r = rects[0];
        assert!(near(r.y0, 19.2 - 6.4, 0.01) && near(r.height(), 1.0, 0.001), "{r:?}");
        assert!(near(r.x0, 0.0, 0.01) && near(r.x1, 36.0, 0.01), "{r:?}");
    }

    #[test]
    fn decoration_follows_span() {
        let mut u = TextSpanStyle::new(2..4);
        u.underline = true;
        let (rects, _) = deco("abcdef", &style("JetBrains Mono", 20.0), &[u]);
        assert_eq!(rects.len(), 1);
        assert!(near(rects[0].x0, 24.0, 0.01) && near(rects[0].x1, 48.0, 0.01), "{:?}", rects[0]);
    }
}
