//! Overlay-family batch: Toast (auto-dismiss via the keyed timer) and Tooltip
//! (hover-delayed passive overlay). Driven headlessly through a real Ui + the
//! animation driver.

use pebbles_core::animation;
use pebbles_core::{IntoWidget, Ui, WidgetExt, component};
use pebbles_foundation::{CrossAxisAlignment, MainAxisSize, Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{Container, OverlayHost, View, column, overlay, text, toast, tooltip};

fn host() -> impl IntoWidget {
    OverlayHost::wrap(text("app"))
}

fn mount() -> (Ui, TextEnv) {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.make_current();
    ui.mount_root(View::new(palette::WHITE, component(host)).boxed());
    ui.layout(&mut env, Size::new(500.0, 400.0));
    (ui, env)
}

#[test]
fn toast_shows_and_auto_dismisses() {
    overlay::init();
    pebbles_core::focus::init();
    animation::reset();

    let (mut ui, mut env) = mount();
    overlay::set_window_size(500.0, 400.0);
    let frame = |ui: &mut Ui, env: &mut TextEnv| {
        ui.rebuild_if_dirty();
        ui.layout(env, Size::new(500.0, 400.0));
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut scene);
    };
    frame(&mut ui, &mut env);

    let id = toast("Saved").description("Draft stored.").duration(0.1).show();
    assert!(toast::any_open(), "toast is showing");
    frame(&mut ui, &mut env); // renders the toast stack — must not panic

    // Auto-dismiss: arm at first tick, fire after the duration.
    animation::tick(0.01);
    assert!(toast::any_open(), "still up before the duration");
    animation::tick(0.20);
    assert!(!toast::any_open(), "auto-dismissed after the duration");
    let _ = id;
}

#[test]
fn toast_manual_dismiss_cancels_timer() {
    overlay::init();
    animation::reset();
    let (mut ui, mut env) = mount();
    let frame = |ui: &mut Ui, env: &mut TextEnv| {
        ui.rebuild_if_dirty();
        ui.layout(env, Size::new(500.0, 400.0));
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut scene);
    };
    frame(&mut ui, &mut env);

    let id = toast("Undo?").action("Undo", || {}).duration(5.0).show();
    assert!(toast::any_open());
    toast::dismiss_toast(id);
    assert!(!toast::any_open(), "manual dismiss removes it immediately");
    // The cancelled timer must not resurrect anything.
    animation::tick(1.0);
    animation::tick(10.0);
    assert!(!toast::any_open());
}

fn tip_root() -> impl IntoWidget {
    // Nest the trigger in a column with a filler below, so the trigger's hit area is a
    // bounded 80×30 at the top-left (not stretched to fill the whole window) — hovering
    // off it then genuinely leaves it.
    OverlayHost::wrap(
        column(pebbles_core::children![
            tooltip(Container::new().width(80.0).height(30.0).child(text("hover me")), "Saved to disk")
                .delay(0.2),
            Container::new().width(240.0).height(220.0),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min),
    )
}

#[test]
fn tooltip_shows_after_hover_delay_and_hides_on_exit() {
    overlay::init();
    pebbles_core::focus::init();
    animation::reset();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let window = Size::new(400.0, 300.0);
    ui.make_current();
    ui.mount_root(View::new(palette::WHITE, component(tip_root)).boxed());
    ui.layout(&mut env, window);
    overlay::set_window_size(400.0, 300.0);
    let frame = |ui: &mut Ui, env: &mut TextEnv| {
        ui.rebuild_if_dirty();
        ui.layout(env, window);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut scene);
    };
    frame(&mut ui, &mut env);

    assert!(!overlay::passive_is_open(), "no tooltip before hover");

    // Hover over the trigger → arms the delay timer.
    ui.dispatch_hover(Offset::new(30.0, 15.0));
    frame(&mut ui, &mut env);
    animation::tick(0.01);
    assert!(!overlay::passive_is_open(), "not yet — before the delay");

    // Past the delay → the tooltip chip appears in the passive layer.
    animation::tick(0.30);
    frame(&mut ui, &mut env);
    assert!(overlay::passive_is_open(), "tooltip shows after the hover delay");

    // Moving off the trigger (down onto the empty filler) hides it.
    ui.dispatch_hover(Offset::new(30.0, 120.0));
    frame(&mut ui, &mut env);
    assert!(!overlay::passive_is_open(), "tooltip hides on hover-exit");
}
