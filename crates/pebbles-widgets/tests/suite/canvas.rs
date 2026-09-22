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
