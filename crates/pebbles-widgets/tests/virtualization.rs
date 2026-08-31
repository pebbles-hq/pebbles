//! Virtualization: the grid's span packing follows the CSS-grid rules (fill
//! left-to-right/top-to-bottom, wrap when a span doesn't fit), and the
//! separated list builds items + separators without touching the disk of
//! layouts beyond the visible window.

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{Container, ListView, View, column, text};

#[test]
fn variable_reversed_and_padded_paint() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 400.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![
                    Container::new()
                        .height(120.0)
                        .child(
                            ListView::variable(20, |i| if i % 2 == 0 { 60.0 } else { 32.0 }, |i| text(format!("v{i}")))
                                .padding(pebbles_foundation::EdgeInsets::all(8.0)),
                        )
                        .into_widget(),
                    Container::new()
                        .height(120.0)
                        .child(ListView::builder(20, 32.0, |i| text(format!("r{i}"))).reverse())
                        .into_widget(),
                    Container::new()
                        .height(120.0)
                        .child(
                            pebbles_widgets::GridView::builder(10, 3, 60.0, |i| text(format!("g{i}")))
                                .spacing(4.0)
                                .padding(pebbles_foundation::EdgeInsets::all(8.0))
                                .reverse(),
                        )
                        .into_widget(),
                ])
            }),
        )
        .into_widget(),
    );
    ui.rebuild_if_dirty();
    ui.layout(&mut env, win);
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut scene);
}

#[test]
fn separated_list_and_spanned_grid_paint() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 400.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![
                    Container::new()
                        .height(140.0)
                        .child(ListView::separated(
                            40,
                            36.0,
                            1.0,
                            |i| text(format!("item {i}")),
                            |_| Container::new().height(1.0).color(palette::BLUE),
                        ))
                        .into_widget(),
                    Container::new()
                        .height(140.0)
                        .child(pebbles_widgets::GridView::builder(12, 4, 80.0, |i| text(format!("#{i}")))
                            .spans(|i| if i == 0 { (2, 2) } else { (1, 1) }))
                        .into_widget(),
                ])
            }),
        )
        .into_widget(),
    );
    ui.rebuild_if_dirty();
    ui.layout(&mut env, win);
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut scene);
}
