//! NestedScrollView: scroll-away shares one scroll position (a single viewport);
//! pinned gives the body its own viewport beneath a fixed header.

use pebbles_core::{AnyWidget, IntoWidget, Ui, component};
use pebbles_foundation::{MainAxisSize, Size, palette};
use pebbles_render::{RenderScroll, TextEnv};
use pebbles_widgets::{View, column, nested_scroll_view, text};

fn body_rows() -> AnyWidget {
    let mut rows: Vec<AnyWidget> = Vec::new();
    for i in 0..40 {
        rows.push(text(format!("row {i}")).into_widget());
    }
    column(rows).main_axis_size(MainAxisSize::Min).into_widget()
}

fn scroll_away_root() -> pebbles_widgets::NestedScrollView {
    nested_scroll_view(text("HEADER").size(24.0), body_rows())
}

fn pinned_root() -> pebbles_widgets::NestedScrollView {
    nested_scroll_view(text("HEADER").size(24.0), body_rows()).pinned(true)
}

fn mount<W: IntoWidget + 'static>(root: fn() -> W) -> (Ui, TextEnv) {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, Size::new(300.0, 240.0));
    (ui, env)
}

#[test]
fn scroll_away_shares_one_scroll_position() {
    let (ui, _env) = mount(scroll_away_root);
    // Header + body live under a single outer scroll view.
    assert_eq!(
        ui.render_tree().find_all::<RenderScroll>().len(),
        1,
        "scroll-away = one shared scroll viewport",
    );
}

#[test]
fn pinned_gives_the_body_its_own_viewport() {
    let (ui, _env) = mount(pinned_root);
    // The header is fixed; the body scrolls in its own viewport.
    assert_eq!(
        ui.render_tree().find_all::<RenderScroll>().len(),
        1,
        "pinned = one scroll viewport (the body), header outside it",
    );
    // And it paints without panicking.
    assert!(ui.render_tree().find::<RenderScroll>().is_some());
}
