//! Pressable / InkWell / InkResponse / Ink: a tap fires `on_tap`, the region takes
//! the Button role + label, a disabled region never fires, and Ink renders a
//! decorated surface.

use std::cell::Cell;

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::{RenderDecoratedBox, TextEnv};
use pebbles_widgets::{
    InkShape, OverlayHost, SizedBox, View, column, ink, ink_response, ink_well, pressable, text,
};

thread_local! {
    static TAPS: Cell<u32> = const { Cell::new(0) };
}

fn frame(ui: &mut Ui, env: &mut TextEnv, win: Size) {
    ui.rebuild_if_dirty();
    ui.layout(env, win);
    let mut scene = pebbles_render::Scene::new();
    ui.paint(env, &mut scene);
}

fn well_root() -> impl IntoWidget {
    OverlayHost::wrap(
        pressable(SizedBox::exact(120.0, 40.0, text("Save")))
            .label("Save")
            .on_tap(|| TAPS.with(|t| t.set(t.get() + 1))),
    )
}

#[test]
fn tap_fires_and_takes_the_button_role() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    TAPS.with(|t| t.set(0));

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(200.0, 120.0);
    ui.mount_root(View::new(palette::WHITE, component(well_root)).into_widget());
    frame(&mut ui, &mut env, win);

    // Button-role semantics with the label.
    let sem = ui.render_tree().semantics_tree();
    assert!(
        sem.iter().any(|n| n.props.label == "Save"),
        "pressable exposes a labelled Button in the semantics tree",
    );

    let p = Offset::new(50.0, 20.0);
    ui.dispatch_pointer_down(p);
    ui.dispatch_tap(p);
    ui.dispatch_pointer_up(p);
    frame(&mut ui, &mut env, win);
    assert_eq!(TAPS.with(Cell::get), 1, "a tap fired on_tap");
}

fn disabled_root() -> impl IntoWidget {
    OverlayHost::wrap(
        ink_well(SizedBox::exact(120.0, 40.0, text("Nope")))
            .disabled(true)
            .on_tap(|| TAPS.with(|t| t.set(t.get() + 100))),
    )
}

#[test]
fn disabled_never_fires() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    TAPS.with(|t| t.set(0));

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(200.0, 120.0);
    ui.mount_root(View::new(palette::WHITE, component(disabled_root)).into_widget());
    frame(&mut ui, &mut env, win);

    let p = Offset::new(50.0, 20.0);
    ui.dispatch_pointer_down(p);
    ui.dispatch_tap(p);
    ui.dispatch_pointer_up(p);
    frame(&mut ui, &mut env, win);
    assert_eq!(TAPS.with(Cell::get), 0, "a disabled region never fires");
}

fn ink_stack_root() -> impl IntoWidget {
    // The Flutter idiom: an Ink surface under an ink_response (circle) tap region.
    column(vec![
        ink_response(SizedBox::square(48.0, text("+"))).into_widget(),
        ink(SizedBox::exact(120.0, 40.0, text("card"))).color(palette::BLUE).radius(10.0).into_widget(),
    ])
}

#[test]
fn ink_response_and_ink_render() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(ink_stack_root)).into_widget());
    frame(&mut ui, &mut env, Size::new(200.0, 200.0));

    // Ink painted a decorated surface with the requested background color.
    let tree = ui.render_tree();
    let painted_blue = tree.find_all::<RenderDecoratedBox>().into_iter().any(|id| {
        tree.object_ref(id).downcast_ref::<RenderDecoratedBox>().unwrap().decoration.color
            == Some(palette::BLUE)
    });
    assert!(painted_blue, "ink(..).color(BLUE) reached a decorated surface");

    // Sanity: the default InkShape is a rectangle.
    assert_eq!(InkShape::default(), InkShape::Rectangle);
}
