//! Glyph runs from a [`TextLayout`] (03 §3.9): backend-neutral `draw_glyphs` ops, one per run
//! of glyphs sharing face, weight, size, synthesis flags and brush slot. Text never
//! rasterizes; the renderer owns the atlas (DR-07).

use kurbo::{Affine, Rect};
use pebbles_foundation::Offset;
use peniko::{Brush, Fill};

use super::TextEnv;
use super::decoration;
use super::layout::TextLayout;
use crate::objects::{ParagraphStyle, TextDecorationStyle, TextSpanStyle};
use crate::paint::{Glyph, Painter};
use crate::tree::PaintCx;

/// What one paint slot (glyph metadata) draws with: the glyph brush and the decoration paint.
/// Paint-only — never part of the shape key (03 §3.9.4).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TextPaint {
    pub(crate) brush: Brush,
    pub(crate) decoration_style: TextDecorationStyle,
    pub(crate) decoration_brush: Brush,
    /// Multiplier of the font's own decoration thickness.
    pub(crate) decoration_thickness: f32,
    /// Glyph shadows, painted under the glyphs in list order.
    pub(crate) shadows: Vec<crate::objects::TextShadow>,
    /// Variable-font axes of this slot (glyph ops carry every axis's coords).
    pub(crate) variations: Vec<crate::objects::FontVariation>,
}

impl TextPaint {
    /// A plain solid paint (decorations in the text color, the font's thickness).
    pub(crate) fn solid(color: peniko::Color) -> Self {
        TextPaint {
            brush: Brush::Solid(color),
            decoration_style: TextDecorationStyle::Solid,
            decoration_brush: Brush::Solid(color),
            decoration_thickness: 1.0,
            shadows: Vec::new(),
            variations: Vec::new(),
        }
    }

    /// Slot 0 of a paragraph.
    pub(crate) fn base(style: &ParagraphStyle) -> Self {
        TextPaint {
            brush: Brush::Solid(style.color),
            decoration_style: style.decoration_style,
            decoration_brush: Brush::Solid(style.decoration_color.unwrap_or(style.color)),
            decoration_thickness: style.decoration_thickness,
            shadows: Vec::new(),
            variations: style.font_variations.clone(),
        }
    }

    /// A span's slot: its overrides over this base.
    pub(crate) fn for_span(&self, span: &TextSpanStyle) -> Self {
        let brush = span.color.map_or_else(|| self.brush.clone(), Brush::Solid);
        // The decoration follows the span's text color unless it sets its own.
        let decoration_brush = match (span.decoration_color, span.color) {
            (Some(c), _) => Brush::Solid(c),
            (None, Some(c)) => Brush::Solid(c),
            (None, None) => self.decoration_brush.clone(),
        };
        TextPaint {
            brush,
            decoration_style: span.decoration_style.unwrap_or(self.decoration_style),
            decoration_brush,
            decoration_thickness: span.decoration_thickness.unwrap_or(self.decoration_thickness),
            shadows: span.shadows.clone(),
            variations: if span.font_variations.is_empty() {
                self.variations.clone()
            } else {
                span.font_variations.clone()
            },
        }
    }
}

/// The faux-italic shear (cosmic-text sets FAKE_ITALIC when the face is upright).
fn fake_italic() -> Affine {
    Affine::skew(14f64.to_radians().tan(), 0.0)
}

/// Paint `layout`'s glyph runs and decorations at `origin` with the slot paints (slot 0 = base).
pub(crate) fn paint_text_layout(
    cx: &mut PaintCx<'_>,
    layout: &TextLayout,
    origin: Offset,
    brushes: &[TextPaint],
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
    brushes: &[TextPaint],
) {
    let transform = Affine::translate((origin.x, origin.y));
    let slot = |metadata: usize| brushes.get(metadata).or(brushes.first());
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
        let runs = line_runs(text, visible, line, origin, brushes, layout.line_text(line));
        // Shadows first, in list order, under every glyph of the line (decorations get none).
        for run in &runs {
            let Some(paint) = slot(run.metadata) else { continue };
            for shadow in &paint.shadows {
                let at = transform * Affine::translate((shadow.offset.x, shadow.offset.y));
                let glyphs = run.glyphs.iter().copied();
                if shadow.blur_sigma > 0.0 {
                    scene.draw_glyphs_blurred(
                        &run.face.font,
                        run.size,
                        &run.face.coords,
                        at,
                        run.glyph_transform,
                        glyphs,
                        shadow.color,
                        shadow.blur_sigma,
                    );
                } else {
                    let brush = Brush::Solid(shadow.color);
                    scene.draw_glyphs(
                        &run.face.font,
                        run.size,
                        &run.face.coords,
                        &brush,
                        at,
                        run.glyph_transform,
                        Fill::NonZero,
                        glyphs,
                    );
                }
            }
        }
        for run in runs {
            let brush = slot(run.metadata).map(|p| p.brush.clone()).unwrap_or_default();
            scene.draw_glyphs(
                &run.face.font,
                run.size,
                &run.face.coords,
                &brush,
                transform,
                run.glyph_transform,
                Fill::NonZero,
                run.glyphs.into_iter(),
            );
            crate::stats::bump_glyph_run();
        }
        // The elided line's glyph bytes index its reshaped text, not the paragraph's attrs.
        let attrs = (!line.from_elided).then(|| &layout.shaped.paras[line.para].attrs);
        paint_decorations(scene, visible, line, attrs, origin, brushes);
    }
}

/// One glyph run of a line, ready to paint.
struct Run {
    face: super::fonts::FaceRef,
    size: f32,
    glyph_transform: Option<Affine>,
    metadata: usize,
    glyphs: Vec<Glyph>,
}

/// The visible runs of `line`: glyphs sharing face, weight, size, synthesis flags and slot.
fn line_runs(
    text: &mut TextEnv,
    visible: Rect,
    line: &super::layout::VisualLine,
    origin: Offset,
    paints: &[TextPaint],
    line_text: &str,
) -> Vec<Run> {
    let glyphs = &line.layout.glyphs;
    let key = |g: &cosmic_text::LayoutGlyph| {
        (g.font_id, g.font_weight.0, g.font_size.to_bits(), g.cache_key_flags.bits(), g.metadata)
    };
    let pen = |g: &cosmic_text::LayoutGlyph| {
        (g.x + g.font_size * g.x_offset, line.baseline + g.y - g.font_size * g.y_offset)
    };
    let mut runs = Vec::new();
    let mut start = 0;
    while start < glyphs.len() {
        let g0 = &glyphs[start];
        let end = glyphs[start..].iter().position(|g| key(g) != key(g0)).map_or(glyphs.len(), |n| start + n);
        let run = &glyphs[start..end];
        start = end;
        if g0.font_size <= 0.0 {
            continue;
        }
        let x0 = run.iter().map(|g| pen(g).0).fold(f32::INFINITY, f32::min);
        let x1 = run.iter().map(|g| pen(g).0 + g.w).fold(f32::NEG_INFINITY, f32::max);
        if origin.x + f64::from(x1) < visible.x0 || origin.x + f64::from(x0) > visible.x1 {
            continue;
        }
        // Inline placeholders are never painted (the widget paints there, T50).
        let run: Vec<&cosmic_text::LayoutGlyph> = run
            .iter()
            .filter(|g| line_text.get(g.start..g.end) != Some(super::shape::OBJECT_REPLACEMENT))
            .collect();
        if run.is_empty() {
            continue;
        }
        let variations = paints.get(g0.metadata).or(paints.first()).map_or(&[][..], |p| &p.variations[..]);
        let Some(face) = text.face_with(g0.font_id, g0.font_weight.0, variations) else { continue };
        runs.push(Run {
            face,
            size: g0.font_size,
            glyph_transform: g0
                .cache_key_flags
                .contains(cosmic_text::CacheKeyFlags::FAKE_ITALIC)
                .then(fake_italic),
            metadata: g0.metadata,
            glyphs: run
                .into_iter()
                .map(|g| {
                    let (x, y) = pen(g);
                    Glyph { id: u32::from(g.glyph_id), x, y }
                })
                .collect(),
        });
    }
    runs
}

/// Span backgrounds (Flutter `TextStyle.background`), painted first: each span's range boxes
/// span the full line box; boxes of one span touching on one line merge (no seams, no padding).
pub(crate) fn paint_backgrounds(
    scene: &mut Painter<'_>,
    visible: Rect,
    layout: &TextLayout,
    origin: Offset,
    spans: &[TextSpanStyle],
) {
    let mut boxes = Vec::new();
    for span in spans {
        let Some(color) = span.background else { continue };
        boxes.clear();
        layout.range_boxes(span.range.clone(), &mut boxes);
        boxes.sort_by(|a, b| a.y0.total_cmp(&b.y0).then(a.x0.total_cmp(&b.x0)));
        let mut merged: Vec<Rect> = Vec::new();
        for r in boxes.iter().copied() {
            match merged.last_mut() {
                Some(m) if m.y0 == r.y0 && r.x0 <= m.x1 + 1e-3 => *m = m.union(r),
                _ => merged.push(r),
            }
        }
        for r in merged {
            let world = r + origin;
            if world.y1 < visible.y0 || world.y0 > visible.y1 {
                continue;
            }
            scene.fill(Fill::NonZero, Affine::IDENTITY, color, None, &world);
        }
    }
}

/// Underline / overline / strikethrough of one line (§3.9.5 positions; T43 styles), never
/// rounded. Thickness = the font's × the slot's multiplier.
///
/// cosmic-text 0.19 merges decoration spans too eagerly: a span is extended by the next glyph
/// with equal decoration data even across undecorated glyphs in between, and its equality
/// ignores our per-slot style / color / thickness. Each of its spans is therefore split into
/// runs of consecutive glyphs that share the paint slot and the decoration actually set on
/// their bytes (`attrs`), so every text span decorates only its own glyphs, in its own style
/// (Flutter / CSS `text-decoration` semantics).
fn paint_decorations(
    scene: &mut Painter<'_>,
    visible: Rect,
    line: &super::layout::VisualLine,
    attrs: Option<&cosmic_text::AttrsList>,
    origin: Offset,
    brushes: &[TextPaint],
) {
    let glyphs = &line.layout.glyphs;
    let transform = Affine::translate((origin.x, origin.y));
    // The decoration in effect on a glyph: its bytes' attrs, or the span's when unknown.
    let deco_of = |g: &cosmic_text::LayoutGlyph, fallback: &cosmic_text::TextDecoration| {
        attrs.map_or(*fallback, |a| a.get_span(g.start).text_decoration)
    };
    for span in &line.layout.decorations {
        let range = span.glyph_range.start.min(glyphs.len())..span.glyph_range.end.min(glyphs.len());
        let mut i = range.start;
        while i < range.end {
            let (slot, deco) = (glyphs[i].metadata, deco_of(&glyphs[i], &span.data.text_decoration));
            let mut j = i + 1;
            while j < range.end
                && glyphs[j].metadata == slot
                && deco_of(&glyphs[j], &span.data.text_decoration) == deco
            {
                j += 1;
            }
            paint_decoration_run(
                scene,
                visible,
                line,
                span,
                &glyphs[i..j],
                &deco,
                transform,
                origin,
                brushes,
            );
            i = j;
        }
    }
}

/// One run of [`paint_decorations`]: glyphs of one slot under one decoration.
#[allow(clippy::too_many_arguments)] // the line, its span and the run, all used
fn paint_decoration_run(
    scene: &mut Painter<'_>,
    visible: Rect,
    line: &super::layout::VisualLine,
    span: &cosmic_text::DecorationSpan,
    run: &[cosmic_text::LayoutGlyph],
    deco: &cosmic_text::TextDecoration,
    transform: Affine,
    origin: Offset,
    brushes: &[TextPaint],
) {
    let has_any = deco.underline != cosmic_text::UnderlineStyle::None || deco.overline || deco.strikethrough;
    let Some(first) = run.first().filter(|_| has_any) else { return };
    let x0 = run.iter().map(|g| g.x).fold(f32::INFINITY, f32::min);
    let x1 = run.iter().map(|g| g.x + g.w).fold(f32::NEG_INFINITY, f32::max);
    if origin.x + f64::from(x1) < visible.x0 || origin.x + f64::from(x0) > visible.x1 {
        return;
    }
    let paint = brushes
        .get(first.metadata)
        .or(brushes.first())
        .cloned()
        .unwrap_or_else(|| TextPaint::solid(peniko::Color::BLACK));
    let size = first.font_size;
    let underline = &span.data.underline_metrics;
    let (x0, x1) = (f64::from(x0), f64::from(x1));
    let mut emit = |top: f32, thickness_em: f32| {
        let t = f64::from(thickness_em * size * paint.decoration_thickness.max(0.0));
        let shapes = decoration::geometry(paint.decoration_style, x0, x1, f64::from(top), t);
        decoration::paint(scene, transform, &shapes, &paint.decoration_brush);
    };
    if deco.underline != cosmic_text::UnderlineStyle::None {
        emit(line.baseline - underline.offset * size, underline.thickness);
    }
    if deco.overline {
        // The band's top at the font's ascent edge, the underline's thickness.
        emit(line.baseline - span.data.ascent * size, underline.thickness);
    }
    if deco.strikethrough {
        let m = &span.data.strikethrough_metrics;
        emit(line.baseline - m.offset * size, m.thickness);
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

    fn brushes() -> Vec<TextPaint> {
        vec![
            TextPaint::solid(Color::BLACK),
            TextPaint::solid(Color::WHITE),
            TextPaint::solid(pebbles_foundation::palette::RED),
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
        assert_eq!(ops[0].5, brushes()[0].brush);
        assert_eq!(ops[1].5, brushes()[2].brush);
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

    /// Two underlined spans with plain text between them: two lines, none under the gap
    /// (cosmic-text 0.19 alone merges them into one line across the gap).
    #[test]
    fn decorations_do_not_bridge_undecorated_text() {
        let mut a = TextSpanStyle::new(0..1);
        a.underline = true;
        let mut b = TextSpanStyle::new(4..5);
        b.underline = true;
        let (rects, _) = deco("abcdef", &style("JetBrains Mono", 20.0), &[a, b]);
        assert_eq!(rects.len(), 2, "{rects:?}");
        assert!(near(rects[0].x0, 0.0, 0.01) && near(rects[0].x1, 12.0, 0.01), "{:?}", rects[0]);
        assert!(near(rects[1].x0, 48.0, 0.01) && near(rects[1].x1, 60.0, 0.01), "{:?}", rects[1]);
    }

    /// Adjacent underlined spans with different styles each keep their own style.
    #[test]
    fn adjacent_decoration_styles_stay_separate() {
        let mut solid = TextSpanStyle::new(0..2);
        solid.underline = true;
        let mut double = TextSpanStyle::new(2..4);
        double.underline = true;
        double.decoration_style = Some(TextDecorationStyle::Double);
        let mut env = TextEnv::new();
        let para = style("JetBrains Mono", 20.0);
        let spans = [solid, double];
        let layout = layout_for_test(&mut env, "abcd", &para, &spans, None);
        let base = TextPaint::base(&para);
        let paints: Vec<TextPaint> =
            std::iter::once(base.clone()).chain(spans.iter().map(|s| base.for_span(s))).collect();
        let mut scene = Scene::new();
        paint_into(&mut Painter::new(&mut scene), &mut env, ALL, &layout, Offset::new(0.0, 0.0), &paints);
        let rects = fills(&scene);
        // One solid line under "ab", two parallel lines under "cd".
        assert_eq!(rects.len(), 3, "{rects:?}");
        assert_eq!(rects.iter().filter(|r| near(r.x0, 0.0, 0.01) && near(r.x1, 24.0, 0.01)).count(), 1);
        assert_eq!(rects.iter().filter(|r| near(r.x0, 24.0, 0.01) && near(r.x1, 48.0, 0.01)).count(), 2);
    }

    /// Fills (rect, brush) painted for `text` with `style` as slot 0.
    fn styled(text: &str, para: &ParagraphStyle) -> (Vec<(Rect, Brush)>, std::rc::Rc<TextLayout>) {
        let mut env = TextEnv::new();
        let layout = layout_for_test(&mut env, text, para, &[], None);
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        paint_into(&mut p, &mut env, ALL, &layout, Offset::new(0.0, 0.0), &[TextPaint::base(para)]);
        let fills = scene
            .ops()
            .iter()
            .filter_map(|op| match op {
                Op::Fill { shape, brush, .. } => Some((shape.bounding_box(), brush.clone())),
                _ => None,
            })
            .collect();
        (fills, layout)
    }

    #[test]
    fn overline_sits_on_ascent() {
        let para = ParagraphStyle { overline: true, ..style("Inter", 16.0) };
        let (fills, layout) = styled("Hi", &para);
        assert_eq!(fills.len(), 1);
        // Inter: ascent 1984 / 2048 em → 15.5 px above the baseline.
        let top = f64::from(layout.first_baseline().unwrap()) - 1984.0 / 2048.0 * 16.0;
        assert!(near(fills[0].0.y0, top, 0.01), "{:?} vs {top}", fills[0].0);
    }

    #[test]
    fn decoration_color_overrides_text_color() {
        let para = ParagraphStyle {
            underline: true,
            decoration_color: Some(pebbles_foundation::palette::RED),
            ..style("Inter", 16.0)
        };
        let (fills, _) = styled("Hi", &para);
        assert_eq!(fills[0].1, Brush::Solid(pebbles_foundation::palette::RED));
        let (plain, _) = styled("Hi", &ParagraphStyle { underline: true, ..style("Inter", 16.0) });
        assert_eq!(plain[0].1, Brush::Solid(Color::BLACK), "defaults to the text color");
    }

    #[test]
    fn thickness_multiplier_scales() {
        let one = styled("Hi", &ParagraphStyle { underline: true, ..style("Inter", 16.0) }).0[0].0;
        let three = styled(
            "Hi",
            &ParagraphStyle { underline: true, decoration_thickness: 3.0, ..style("Inter", 16.0) },
        )
        .0[0]
            .0;
        assert!(near(three.height(), one.height() * 3.0, 0.001), "{one:?} {three:?}");
        assert!(near(three.y0, one.y0, 0.001), "the band keeps its top");
    }

    #[test]
    fn dashed_style_reaches_the_painter() {
        let para = ParagraphStyle {
            underline: true,
            decoration_style: TextDecorationStyle::Dashed,
            ..style("JetBrains Mono", 20.0)
        };
        // 4 glyphs = 48 px at t = 1 px: dashes every 5 px.
        assert_eq!(styled("abcd", &para).0.len(), 10);
    }

    fn shadow(sigma: f32) -> crate::objects::TextShadow {
        crate::objects::TextShadow { offset: Offset::new(2.0, 3.0), blur_sigma: sigma, color: Color::BLACK }
    }

    /// Op kinds in paint order for "ab" whose span 0..2 carries `shadows`.
    fn shadow_ops(shadows: Vec<crate::objects::TextShadow>) -> Vec<&'static str> {
        let mut env = TextEnv::new();
        let mut span = TextSpanStyle::new(0..2);
        span.shadows = shadows;
        let st = style("Inter", 16.0);
        let layout = layout_for_test(&mut env, "ab", &st, std::slice::from_ref(&span), None);
        let base = TextPaint::base(&st);
        let paints = [base.clone(), base.for_span(&span)];
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        paint_into(&mut p, &mut env, ALL, &layout, Offset::new(0.0, 0.0), &paints);
        scene
            .ops()
            .iter()
            .map(|op| match op {
                Op::Glyphs { transform, .. } if *transform != Affine::IDENTITY => "shadow",
                Op::Glyphs { .. } => "glyphs",
                Op::BlurredGlyphs { .. } => "blurred",
                _ => "other",
            })
            .collect()
    }

    #[test]
    fn shadow_is_painted_before_glyphs() {
        assert_eq!(shadow_ops(vec![shadow(2.0)]), ["blurred", "glyphs"]);
        assert_eq!(shadow_ops(vec![shadow(2.0), shadow(0.0)]), ["blurred", "shadow", "glyphs"]);
    }

    #[test]
    fn zero_sigma_uses_plain_glyphs() {
        assert_eq!(shadow_ops(vec![shadow(0.0)]), ["shadow", "glyphs"]);
    }

    /// Background fills for `text` with span `range` on `background`.
    fn backgrounds(
        text: &str,
        range: std::ops::Range<usize>,
        width: Option<f32>,
    ) -> (Vec<Rect>, std::rc::Rc<TextLayout>) {
        let mut env = TextEnv::new();
        let mut span = TextSpanStyle::new(range);
        span.background = Some(pebbles_foundation::palette::RED);
        let layout = layout_for_test(&mut env, text, &style("JetBrains Mono", 20.0), &[], width);
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        paint_backgrounds(&mut p, ALL, &layout, Offset::new(0.0, 0.0), &[span]);
        (fills(&scene), layout)
    }

    #[test]
    fn background_covers_line_box() {
        let (rects, layout) = backgrounds("abcdef", 1..4, None);
        assert_eq!(rects.len(), 1);
        let r = rects[0];
        assert!(near(r.x0, 12.0, 0.01) && near(r.x1, 48.0, 0.01), "{r:?}");
        assert!(
            near(r.y0, 0.0, 0.01) && near(r.y1, f64::from(layout.lines[0].height), 0.01),
            "full line box: {r:?}"
        );
    }

    #[test]
    fn background_spans_wrap_per_line() {
        let (rects, _) = backgrounds("aaaa bbbb cccc", 2..12, Some(100.0));
        assert_eq!(rects.len(), 3, "{rects:?}");
        let mut ys: Vec<i64> = rects.iter().map(|r| r.y0 as i64).collect();
        ys.dedup();
        assert_eq!(ys.len(), 3);
    }
}
