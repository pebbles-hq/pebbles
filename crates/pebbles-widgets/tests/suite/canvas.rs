//! H2: the `canvas` widget runs its painter at paint time and honors its size. Driven
//! headlessly through a real Ui + a CPU scene (no GPU).

use std::cell::Cell;

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Offset, Rect, Size, palette};
use pebbles_render::{BezPath, Canvas, Gradient, RenderCanvas, TextEnv};
use pebbles_widgets::{View, canvas, center};

thread_local! {
    static PAINTS: Cell<u32> = const { Cell::new(0) };
}

fn root() -> impl IntoWidget {
    // center() loosens the root View's tight window constraints — a SizedBox
    // under TIGHT constraints is forced to the parent's size (Flutter
    // semantics), so the explicit 120×80 only holds under loose ones.
    center(
        canvas(|c: &mut Canvas<'_>| {
            PAINTS.with(|p| p.set(p.get() + 1));
            let s = c.size();
            c.fill_circle(Offset::new(s.width / 2.0, s.height / 2.0), 10.0, palette::BLUE);
        })
        .width(120.0)
        .height(80.0),
    )
}

#[test]
fn canvas_painter_runs_and_honors_size() {
    PAINTS.with(|p| p.set(0));

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, Size::new(400.0, 400.0));

    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut env, &mut scene);
    assert!(PAINTS.with(Cell::get) >= 1, "the painter ran during paint");

    // The explicit width/height are honored (RenderCanvas sits in a SizedBox).
    let tree = ui.render_tree();
    let id = tree.find::<RenderCanvas>().expect("canvas render node present");
    assert_eq!(tree.size_of(id).width, 120.0);
    assert_eq!(tree.size_of(id).height, 80.0);
}

/// A painter that exercises the non-trivial Canvas verbs beyond `fill_circle`: a
/// filled BezPath, a gradient rect + gradient path fill, and dashed line + path
/// strokes. The point is that each verb lays out + paints without panicking.
fn primitives() -> impl IntoWidget {
    center(
        canvas(|c: &mut Canvas<'_>| {
            PAINTS.with(|p| p.set(p.get() + 1));
            let s = c.size();

            // A filled arbitrary path (a triangle).
            let mut tri = BezPath::new();
            tri.move_to((0.0, 0.0));
            tri.line_to((s.width, 0.0));
            tri.line_to((s.width / 2.0, s.height));
            tri.close_path();
            c.fill_path(&tri, palette::BLUE);

            // Gradient fills — a rect and the same path.
            let grad = Gradient::radial(s.width.max(s.height), [palette::BLUE, palette::WHITE]);
            c.fill_rect_gradient(Rect::new(0.0, 0.0, s.width, s.height), &grad);
            c.fill_path_gradient(&tri, &grad);

            // Dashed strokes — a straight line and the path outline.
            c.stroke_line_dashed(
                Offset::new(0.0, 0.0),
                Offset::new(s.width, s.height),
                2.0,
                palette::BLACK,
                &[6.0, 4.0],
                0.0,
            );
            c.stroke_path_dashed(&tri, 1.5, palette::BLACK, &[3.0, 3.0], 1.0);
        })
        .width(120.0)
        .height(80.0),
    )
}

#[test]
fn canvas_primitives_lay_out_and_paint_without_panic() {
    PAINTS.with(|p| p.set(0));

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(primitives)).into_widget());
    ui.layout(&mut env, Size::new(400.0, 400.0));

    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut env, &mut scene);
    assert!(PAINTS.with(Cell::get) >= 1, "the primitives painter ran during paint");

    // The canvas still sizes to its explicit box after all the drawing verbs.
    let tree = ui.render_tree();
    let id = tree.find::<RenderCanvas>().expect("canvas render node present");
    assert_eq!(tree.size_of(id).width, 120.0);
    assert_eq!(tree.size_of(id).height, 80.0);
}

// ----- text on the canvas (Gravel terminal) --------------------------------

thread_local! {
    static TEXT_DRAWN: std::cell::RefCell<Vec<Size>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn mono_style() -> pebbles_render::ParagraphStyle {
    pebbles_render::ParagraphStyle {
        font_size: 13.0,
        line_height: 1.3,
        font_family: Some("JetBrains Mono".into()),
        soft_wrap: false,
        ..Default::default()
    }
}

fn text_canvas() -> impl IntoWidget {
    center(
        canvas(|c: &mut Canvas<'_>| {
            let style = mono_style();
            let drawn = c.draw_text("ls -la", &style, Offset::new(4.0, 2.0));
            let measured = c.measure_text("ls -la", &style, None);
            let cell = c.monospace_cell("JetBrains Mono", 13.0, 1.3);
            let spans = [pebbles_render::TextSpanStyle {
                color: Some(palette::RED),
                ..pebbles_render::TextSpanStyle::new(0..2)
            }];
            let para = c.draw_paragraph("ok done", &spans, &style, Offset::new(0.0, 20.0), Some(200.0));
            TEXT_DRAWN.with(|t| *t.borrow_mut() = vec![drawn, measured, cell, para]);
        })
        .width(300.0)
        .height(80.0),
    )
}

/// Regression (Gravel terminal): the canvas had no text API (it rendered every row as a
/// widget), and there was no public way to measure a monospace cell — the terminal
/// hard-coded JetBrains Mono's 0.6 em advance.
#[test]
fn canvas_draws_and_measures_text_like_the_text_widget() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(text_canvas)).into_widget());
    ui.layout(&mut env, Size::new(400.0, 400.0));
    pebbles_render::stats::reset_frame();
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut env, &mut scene);
    #[cfg(debug_assertions)]
    assert!(pebbles_render::stats::glyph_runs() >= 2, "both strings emitted glyph runs");

    let [drawn, measured, cell, para] =
        TEXT_DRAWN.with(|t| <[Size; 4]>::try_from(t.borrow().clone()).expect("painter ran"));
    assert_eq!(drawn, measured, "measure_text predicts draw_text");
    // JetBrains Mono advances 0.6 em; one line is font_size × line_height.
    assert!((cell.width - 13.0 * 0.6).abs() < 0.05, "cell width {}", cell.width);
    assert!((cell.height - 13.0 * 1.3).abs() < 0.5, "cell height {}", cell.height);
    assert!((drawn.width - 6.0 * cell.width).abs() < 0.05, "6 cells wide: {drawn:?}");
    assert_eq!(drawn.height, cell.height);
    assert!(para.width > 0.0 && para.height == cell.height);
    // The context-free measurement agrees with the window's environment.
    assert_eq!(pebbles_render::monospace_cell("JetBrains Mono", 13.0, 1.3), cell);
    assert_eq!(pebbles_render::measure_text("ls -la", &mono_style(), None), measured);

    // And with a `Text` widget in the same style.
    let mut env2 = TextEnv::new();
    let layout = pebbles_render::text::layout_for_test(&mut env2, "ls -la", &mono_style(), &[], None);
    assert_eq!(f64::from(layout.width()), measured.width);
}
