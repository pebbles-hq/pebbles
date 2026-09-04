//! Navigation customization guards: Tabs, Accordion, Breadcrumb and Menubar
//! accept a Style — the surface (background/radius) reaches the render
//! decoration, custom separators render, and everything paints.

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Size, palette};
use pebbles_render::{IconKind, RenderDecoratedBox, TextEnv};
use pebbles_testing::draw_frame as frame;
use pebbles_widgets::{View, accordion, breadcrumb, column, menu_item, menubar, style, tabs, text};

#[test]
fn tabs_style_lands_on_the_strip() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(500.0, 200.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![
                    tabs(0usize)
                        .tab("One", text(""), || {})
                        .tab("Two", text(""), || {})
                        .style(style().background(palette::BLUE).radius_all(0.0))
                        .into_widget(),
                ])
                .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start)
            }),
        )
        .into_widget(),
    );
    frame(&mut ui, &mut env, win);

    let tree = ui.render_tree();
    let rid = tree.find::<RenderDecoratedBox>().expect("the styled strip");
    let deco = tree.object_ref(rid).downcast_ref::<RenderDecoratedBox>().expect("decorated");
    assert_eq!(deco.decoration.color, Some(palette::BLUE), "style background lands on the strip");
}

#[test]
fn accordion_style_lands_on_the_surface() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(500.0, 200.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                accordion()
                    .item("One", text(""))
                    .item("Two", text(""))
                    .style(style().background(palette::BLUE).radius_all(0.0))
                    .into_widget()
            }),
        )
        .into_widget(),
    );
    frame(&mut ui, &mut env, win);

    let tree = ui.render_tree();
    let rid = tree.find::<RenderDecoratedBox>().expect("the styled surface");
    let deco = tree.object_ref(rid).downcast_ref::<RenderDecoratedBox>().expect("decorated");
    assert_eq!(deco.decoration.color, Some(palette::BLUE), "style background lands on the surface");
    assert_eq!(
        deco.decoration.radius,
        pebbles_render::BorderRadius::all(0.0),
        "style radius lands on the surface"
    );
}

#[test]
fn breadcrumb_separator_and_style_paint() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(500.0, 200.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![
                    breadcrumb(vec!["A".into(), "B".into(), "C".into()])
                        .separator(IconKind::Dot)
                        .style(style().color(palette::BLUE).font_size(16.0))
                        .into_widget(),
                    breadcrumb(["a", "b", "c", "d", "e", "f"].into_iter().map(String::from).collect())
                        .max_visible(3)
                        .into_widget(),
                ])
                .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start)
            }),
        )
        .into_widget(),
    );
    frame(&mut ui, &mut env, win);
}

#[test]
fn menubar_style_paints() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(500.0, 200.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                menubar()
                    .menu("File", [menu_item("New"), menu_item("Open")])
                    .menu("Edit", [menu_item("Undo")])
                    .style(style().background(palette::BLUE).color(palette::WHITE))
                    .into_widget()
            }),
        )
        .into_widget(),
    );
    frame(&mut ui, &mut env, win);

    let tree = ui.render_tree();
    let rid = tree.find::<RenderDecoratedBox>().expect("the styled bar");
    let deco = tree.object_ref(rid).downcast_ref::<RenderDecoratedBox>().expect("decorated");
    assert_eq!(deco.decoration.color, Some(palette::BLUE), "style background lands on the bar");
}
