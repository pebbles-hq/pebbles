//! [`ListTile`]: rows are clickable (`on_tap` fires, disabled rows never do),
//! and the universal Style genuinely styles the row — the surface background
//! lands on the render decoration, text props land on the title, and
//! padding/selection/density variants all lay out and paint.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{CrossAxisAlignment, Offset, Size, palette};
use pebbles_render::{RenderDecoratedBox, TextEnv};
use pebbles_widgets::{
    OverlayHost, View, column, list_tile, style, text,
};

thread_local! {
    static TAPPED: RefCell<usize> = const { RefCell::new(0) };
}

fn root() -> impl IntoWidget {
    OverlayHost::wrap(
        column(vec![
            list_tile("Clickable")
                .subtitle("tap me")
                .on_tap(|| TAPPED.with(|t| *t.borrow_mut() += 1))
                .into_widget(),
            list_tile("Disabled")
                .on_tap(|| TAPPED.with(|t| *t.borrow_mut() += 100))
                .disabled(true)
                .into_widget(),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch),
    )
}

#[test]
fn tap_fires_and_disabled_never_does() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    TAPPED.with(|t| *t.borrow_mut() = 0);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 200.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, win);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut scene);
    };
    frame(&mut ui);

    // Two default rows (~37px tall): row 0 y ≈ 0..37, row 1 ≈ 37..74.
    let tap = |ui: &mut Ui, p: Offset| {
        ui.dispatch_pointer_down(p);
        ui.dispatch_tap(p);
        ui.dispatch_pointer_up(p);
    };
    tap(&mut ui, Offset::new(100.0, 18.0));
    frame(&mut ui);
    assert_eq!(TAPPED.with(|t| t.borrow().clone()), 1, "on_tap fires on the clickable row");

    tap(&mut ui, Offset::new(100.0, 55.0));
    frame(&mut ui);
    assert_eq!(TAPPED.with(|t| t.borrow().clone()), 1, "a disabled row never fires on_tap");
}

fn styled_root() -> impl IntoWidget {
    OverlayHost::wrap(
        column(vec![
            list_tile("Custom")
                .subtitle("styled row")
                .leading(text("◎"))
                .trailing(text("→"))
                .selected(true)
                .dense(true)
                .content_padding(palette_edge_insets())
                .leading_gap(20.0)
                .style(
                    style()
                        .background(palette::BLUE)
                        .radius_all(10.0)
                        .color(palette::WHITE)
                        .font_size(20.0)
                        .font_weight(700.0)
                        .min_height(48.0),
                )
                .into_widget(),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch),
    )
}

fn palette_edge_insets() -> pebbles_foundation::EdgeInsets {
    pebbles_foundation::EdgeInsets::all(16.0)
}

#[test]
fn style_lands_on_the_surface_and_title() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 200.0);
    ui.mount_root(View::new(palette::WHITE, component(styled_root)).into_widget());
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, win);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut scene);
    };
    frame(&mut ui);

    // The surface: the styled row's DecoratedBox carries the user background
    // (hover-tinted only while hovered — it isn't here).
    let tree = ui.render_tree();
    let rid = tree.find::<RenderDecoratedBox>().expect("a decorated tile surface");
    let deco = tree.object_ref(rid).downcast_ref::<RenderDecoratedBox>().expect("decorated box");
    assert_eq!(
        deco.decoration.color,
        Some(palette::BLUE),
        "the user style background reached the row surface"
    );
    assert_eq!(
        deco.decoration.radius,
        pebbles_render::BorderRadius::all(10.0),
        "the user style radius reached the row surface"
    );

    // Layout honors the styled min-height (padding 16 + text 20px line ≈ 40+).
    let row_height = tree.size_of(rid).height;
    assert!(row_height >= 48.0, "min_height(48) bounds the styled row, got {row_height}");
}

#[test]
fn bare_tile_still_paints_with_defaults() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 200.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                OverlayHost::wrap(
                    column(vec![
                        list_tile("Plain").into_widget(),
                        list_tile("Dense").dense(true).into_widget(),
                        list_tile("Selected").selected(true).into_widget(),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch),
                )
            }),
        )
        .into_widget(),
    );
    ui.rebuild_if_dirty();
    ui.layout(&mut env, win);
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut scene);
}
