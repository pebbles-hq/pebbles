//! H2: the `canvas` widget runs its painter at paint time and honors its size. Driven
//! headlessly through a real Ui + a CPU scene (no GPU).

use std::cell::Cell;

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::{Canvas, RenderCanvas, TextEnv};
use pebbles_widgets::{View, canvas};

thread_local! {
    static PAINTS: Cell<u32> = const { Cell::new(0) };
}

fn root() -> impl IntoWidget {
    canvas(|c: &mut Canvas<'_>| {
        PAINTS.with(|p| p.set(p.get() + 1));
        let s = c.size();
        c.fill_circle(Offset::new(s.width / 2.0, s.height / 2.0), 10.0, palette::BLUE);
    })
    .width(120.0)
    .height(80.0)
}

#[test]
fn canvas_painter_runs_and_honors_size() {
    PAINTS.with(|p| p.set(0));

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, Size::new(400.0, 400.0));

    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut scene);
    assert!(PAINTS.with(Cell::get) >= 1, "the painter ran during paint");

    // The explicit width/height are honored (RenderCanvas sits in a SizedBox).
    let tree = ui.render_tree();
    let id = tree.find::<RenderCanvas>().expect("canvas render node present");
    assert_eq!(tree.size_of(id).width, 120.0);
    assert_eq!(tree.size_of(id).height, 80.0);
}
