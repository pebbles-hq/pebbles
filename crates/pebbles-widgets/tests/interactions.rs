//! Coverage for the new interactive pieces: RadioGroup selection, Resizable
//! drag-to-resize, the Dialog request queue, and the overlay scroll-follow helpers.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use pebbles_core::{IntoWidget, Signal, Ui, WidgetExt, component, create_signal};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{Container, OverlayHost, View, column, radio_group, resizable, text};

// ---------------------------------------------------------------------------
// RadioGroup
// ---------------------------------------------------------------------------

thread_local! {
    static PICKED: Cell<usize> = const { Cell::new(usize::MAX) };
}

fn radio_root() -> impl IntoWidget {
    column(vec![
        radio_group(["Alpha", "Beta", "Gamma"])
            .value(0)
            .on_changed(|i| PICKED.with(|c| c.set(i)))
            .into_widget(),
    ])
    .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start)
}

#[test]
fn radio_group_selects_on_tap() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut text_env = TextEnv::new();
    let window = Size::new(300.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(radio_root)).boxed());
    ui.layout(&mut text_env, window);

    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut text_env, window);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut scene);
    };
    frame(&mut ui);

    // Rows stack vertically with 12px spacing; the 18px radios put row 1 (Beta)
    // around y = 30..48. Tap its control.
    let beta = Offset::new(9.0, 39.0);
    ui.dispatch_pointer_down(beta);
    ui.dispatch_tap(beta);
    ui.dispatch_pointer_up(beta);
    frame(&mut ui);
    assert_eq!(PICKED.with(Cell::get), 1, "tapping the second row selects index 1");
}

// ---------------------------------------------------------------------------
// Resizable
// ---------------------------------------------------------------------------

fn resizable_root() -> impl IntoWidget {
    let _ = create_signal(0i32);
    column(vec![
        Container::new()
            .height(100.0)
            .child(
                resizable(vec![
                    Container::new().color(palette::BLUE).into_widget(),
                    Container::new().color(palette::GREEN).into_widget(),
                ])
                .length(400.0)
                .min(50.0),
            )
            .into_widget(),
    ])
    .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start)
    .into_widget()
}

#[test]
fn resizable_handle_drags() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut text_env = TextEnv::new();
    let window = Size::new(500.0, 200.0);
    ui.mount_root(View::new(palette::WHITE, component(resizable_root)).boxed());
    ui.layout(&mut text_env, window);
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut text_env, window);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut scene);
    };
    frame(&mut ui);

    // Two 200px panels → the handle sits at x ≈ 200 (y anywhere in 0..100).
    let handle_pt = Offset::new(200.0, 50.0);
    let src = ui.pan_target_at(handle_pt).expect("a pan handle at the split");
    ui.dispatch_pan_start(src, handle_pt);
    ui.dispatch_pan_update(src, Offset::new(280.0, 50.0));
    frame(&mut ui);
    // The handle should have followed the drag ~80px to the right.
    assert!(
        ui.pan_target_at(Offset::new(280.0, 50.0)).is_some(),
        "the handle moved with the drag (panel grew)"
    );
    assert!(
        ui.pan_target_at(Offset::new(200.0, 50.0)).is_none(),
        "the handle is no longer at its old position"
    );
}

// ---------------------------------------------------------------------------
// Dialog (in-app modal)
// ---------------------------------------------------------------------------

#[test]
fn dialog_modal_opens_closes_and_paints() {
    use pebbles_widgets::dialog;
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    dialog::init();

    let mut ui = Ui::new();
    let mut text_env = TextEnv::new();
    let window = Size::new(500.0, 400.0);
    ui.mount_root(View::new(palette::WHITE, OverlayHost::wrap(text("app behind the modal"))).boxed());
    ui.layout(&mut text_env, window);
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut text_env, window);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut scene);
    };
    frame(&mut ui);

    let fired = Rc::new(Cell::new(false));
    let f2 = fired.clone();
    let id = dialog::dialog(text("Hello from a modal dialog"))
        .title("Greetings")
        .width(300.0)
        .on_close(move || f2.set(true))
        .open();
    assert!(dialog::is_open(), "open() shows the dialog");
    frame(&mut ui); // renders scrim + centered surface — must not panic

    // A close for the wrong id is ignored.
    dialog::close_dialog(id + 999);
    assert!(dialog::is_open());

    // Escape / outside-click path.
    dialog::dismiss_top();
    assert!(!dialog::is_open(), "dismiss closes a dismissible dialog");
    assert!(fired.get(), "on_close fired");
    frame(&mut ui); // renders again with no modal
}

// ---------------------------------------------------------------------------
// Window open/close queue + IPC channel
// ---------------------------------------------------------------------------

#[test]
fn window_open_and_close_enqueue() {
    use pebbles_widgets::window;
    let id = window::window(text("inspector")).title("Inspector").size(500, 400).open();
    let open = window::take_open_requests();
    assert_eq!(open.len(), 1);
    assert_eq!(open[0].id, id);
    assert_eq!(open[0].title, "Inspector");
    assert_eq!((open[0].width, open[0].height), (500, 400));
    assert!(window::take_open_requests().is_empty());

    window::close_window(id);
    assert_eq!(window::take_close_requests(), vec![id]);
}

// ---------------------------------------------------------------------------
// Hooks-order guardrail (debug-only)
// ---------------------------------------------------------------------------

thread_local! {
    static REV: RefCell<Option<Signal<i32>>> = const { RefCell::new(None) };
}
fn rev() -> Signal<i32> {
    REV.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            *c = Some(create_signal(0));
        }
        c.unwrap()
    })
}

/// Violates the hooks rule on purpose: the local signal at position 0 is `i32` on the
/// first render and `&str` on the second — the guardrail must catch the type change.
fn hooks_violator() -> impl IntoWidget {
    let r = rev().get(); // subscribe to a global signal so a write re-renders us
    if r == 0 {
        let _ = create_signal(0i32);
    } else {
        let _ = create_signal("x");
    }
    text("x")
}

#[test]
#[should_panic(expected = "hooks rule")]
fn hooks_rule_violation_is_caught() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = rev(); // create the global signal BEFORE mount (app scope, not owned)
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(hooks_violator)).boxed());
    ui.layout(&mut env, Size::new(100.0, 100.0)); // render 1: position 0 = i32
    rev().set(1); // mark the component dirty
    ui.rebuild_if_dirty(); // render 2: position 0 = &str → guardrail panics
}

#[test]
fn modifier_ext_chains_and_renders() {
    use pebbles_widgets::ModifierExt;
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    // SwiftUI-style child-first chain — compiles (trait on any widget) and paints.
    let root = text("hi").clipped(8.0).padded(12.0).sized(120.0, 40.0).opacity(0.9).centered();
    ui.mount_root(View::new(palette::WHITE, root).boxed());
    ui.layout(&mut env, Size::new(240.0, 160.0));
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut scene); // must not panic
}

#[test]
fn into_children_accepts_tuples_vecs_arrays_options() {
    use pebbles_core::widget::IntoChildren;
    let t = || text("x");
    assert_eq!((t(), t(), t()).into_children().len(), 3, "3-tuple");
    assert_eq!((t(),).into_children().len(), 1, "1-tuple");
    assert_eq!(vec![t(), t()].into_children().len(), 2, "Vec");
    assert_eq!([t(), t(), t(), t()].into_children().len(), 4, "array");
    assert_eq!(Some(t()).into_children().len(), 1, "Some");
    assert_eq!(None::<pebbles_widgets::Text>.into_children().len(), 0, "None");
    assert_eq!(().into_children().len(), 0, "unit = no children");
    // heterogeneous tuple (different widget types) compiles + collects:
    assert_eq!((text("a"), Container::new(), text("b")).into_children().len(), 3, "heterogeneous");
}

#[test]
fn channel_carries_typed_messages() {
    use pebbles_core::channel;
    let ch = channel::<i32>();
    assert_eq!(ch.peek(), None);
    ch.send(7);
    assert_eq!(ch.peek(), Some(7));
    ch.send(42);
    assert_eq!(ch.peek(), Some(42), "latest message wins");

    // `on` delivers the current message immediately on subscribe (create_effect runs
    // the handler once), then on each subsequent send.
    let seen = Rc::new(Cell::new(0));
    let s2 = seen.clone();
    ch.on(move |m| s2.set(m));
    assert_eq!(seen.get(), 42, "on() delivered the latest message");
}

#[test]
fn dialog_non_dismissible_ignores_dismiss() {
    use pebbles_widgets::dialog;
    dialog::init();
    let id = dialog::dialog(text("must choose")).dismissible(false).open();
    dialog::dismiss_top();
    assert!(dialog::is_open(), "a non-dismissible dialog ignores Escape/outside-click");
    dialog::close_dialog(id); // explicit close still works
    assert!(!dialog::is_open());
}

// ---------------------------------------------------------------------------
// Overlay scroll-follow helpers
// ---------------------------------------------------------------------------

#[test]
fn overlay_shift_and_over_panel() {
    use pebbles_widgets::overlay;
    overlay::init();

    overlay::show_overlay(pebbles_widgets::text("menu").into_widget(), 100.0, 100.0, 50.0, 40.0);
    assert!(overlay::over_panel(120.0, 110.0), "point inside the panel");
    assert!(!overlay::over_panel(10.0, 10.0), "point outside the panel");

    // Following a page scroll up by 20px moves the panel up by 20px.
    overlay::shift(0.0, -20.0);
    assert!(overlay::over_panel(120.0, 90.0), "panel followed the shift (top now 80)");
    assert!(!overlay::over_panel(120.0, 130.0), "below the shifted panel");

    overlay::hide_overlay();
    assert!(!overlay::over_panel(120.0, 90.0), "nothing open");
}
