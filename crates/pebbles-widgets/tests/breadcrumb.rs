//! [`Breadcrumb`]: `max_visible` collapses the middle segments into a "…" dropdown
//! that lists the hidden ones — and the trail paints.

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{CrossAxisAlignment, Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{OverlayHost, View, breadcrumb, column, overlay};

fn crumbs() -> impl IntoWidget {
    OverlayHost::wrap(
        column(vec![
            breadcrumb(
                ["Home", "Workspace", "Design", "Components", "Input", "Select", "pebbles.rs"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
            )
            .max_visible(4)
            .into_widget(),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start),
    )
}

#[test]
fn ellipsis_opens_a_menu_of_hidden_segments() {
    overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(500.0, 200.0);
    ui.mount_root(View::new(palette::WHITE, component(crumbs)).into_widget());
    ui.layout(&mut env, win);
    ui.rebuild_if_dirty();
    ui.layout(&mut env, win);
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut scene);

    assert!(!overlay::is_open(), "closed initially");

    // Visible trail: Home › … › Select › pebbles.rs. The "…" sits right of
    // "Home" (13px text ≈ 36px + two 6px gaps + a 14px chevron ≈ 62px in).
    let ellipsis = Offset::new(70.0, 9.0);
    ui.dispatch_pointer_down(ellipsis);
    ui.dispatch_tap(ellipsis);
    ui.dispatch_pointer_up(ellipsis);
    ui.rebuild_if_dirty();
    ui.layout(&mut env, win);
    ui.paint(&mut scene);
    assert!(overlay::is_open(), "clicking the … opens the hidden-segments menu");

    overlay::hide_overlay();
    ui.rebuild_if_dirty();
    ui.layout(&mut env, win);
    ui.paint(&mut scene);
    assert!(!overlay::is_open(), "and it dismisses");
}
