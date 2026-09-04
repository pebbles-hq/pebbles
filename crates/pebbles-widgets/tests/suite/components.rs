//! Smoke + interaction coverage for the surface/disclosure components: a
//! self-managing Collapsible actually toggles, and Card (header/footer), Avatar
//! (status dot), AvatarGroup (overflow) and ButtonGroup (joined, clipped) all
//! reconcile, lay out and PAINT without panicking.

use std::cell::Cell;

use pebbles_core::{IntoWidget, Ui, component, create_signal};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{
    Avatar, ButtonVariant, View, avatar, avatar_group, body, button, button_group, card, checkbox,
    collapsible, column, empty, kbd, scroll_area, skeleton, text,
};

thread_local! {
    static OPEN: Cell<bool> = const { Cell::new(false) };
}

fn gallery() -> impl IntoWidget {
    let _ = create_signal(0i32);
    let people: Vec<Avatar> = ["RS", "AK", "JB", "CV", "MK"]
        .into_iter()
        .map(|i| avatar(i).color(palette::BLUE))
        .collect();

    column(vec![
        collapsible("Toggle details", body("hidden content"))
            .on_toggle(|o| OPEN.with(|c| c.set(o)))
            .into_widget(),
        card()
            .title("Create project")
            .description("Deploy in one click.")
            .footer(button("Save").variant(ButtonVariant::Secondary))
            .into_widget(),
        avatar("RS").status(palette::GREEN).into_widget(),
        avatar_group(people).max(3).into_widget(),
        button_group(vec![
            button("Left").variant(ButtonVariant::Secondary),
            button("Center").variant(ButtonVariant::Secondary),
            button("Right").variant(ButtonVariant::Secondary),
        ])
        .into_widget(),
        kbd("⌘K").into_widget(),
        empty().icon(pebbles_render::lucide::SEARCH).title("Nothing here").into_widget(),
        scroll_area(column(pebbles_core::children![text("a"), text("b"), text("c")]))
            .width(120.0)
            .height(60.0)
            .into_widget(),
        checkbox(false).indeterminate(true).label("Mixed").into_widget(),
        skeleton(120.0, 12.0).shimmer().into_widget(),
    ])
    .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start)
}

#[test]
fn surfaces_and_disclosure_paint_and_toggle() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut text_env = TextEnv::new();
    let window = Size::new(500.0, 600.0);
    ui.mount_root(View::new(palette::WHITE, component(gallery)).into_widget());
    ui.layout(&mut text_env, window);

    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut text_env, window);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut text_env, &mut scene);
    };
    frame(&mut ui);

    // The collapsible header is the first thing in the column (top-left).
    let header = Offset::new(40.0, 14.0);
    assert!(!OPEN.with(|c| c.get()), "starts closed");
    ui.dispatch_pointer_down(header);
    ui.dispatch_tap(header);
    ui.dispatch_pointer_up(header);
    frame(&mut ui);
    assert!(OPEN.with(|c| c.get()), "tapping the header opens it");

    // Tapping again closes it — and everything still paints.
    ui.dispatch_pointer_down(header);
    ui.dispatch_tap(header);
    ui.dispatch_pointer_up(header);
    frame(&mut ui);
    assert!(!OPEN.with(|c| c.get()), "tapping again closes it");
}
