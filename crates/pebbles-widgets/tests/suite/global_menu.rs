//! The global right-click menu: the standard fallback opens when nothing claims
//! a right-click, its items route clipboard/selection intents to the focused
//! editor (disabling themselves without one), it can be turned off, its options
//! replaced, its surface styled, and suppressed per area.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, KeyInput, Ui, component};
use pebbles_foundation::{Size, palette};
use pebbles_render::TextEnv;
use pebbles_testing::draw_frame as frame;
use pebbles_widgets::{
    OverlayHost, View, block_context_menu, column, menu_item, overlay, reset_global_menu, set_global_menu,
    set_global_menu_enabled, set_global_menu_style, style, text, text_field,
};

thread_local! {
    static PICKED: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn init() {
    overlay::init();
    pebbles_core::focus::init();
    set_global_menu_enabled(true);
    reset_global_menu();
}

#[test]
fn show_opens_and_custom_options_replace() {
    init();
    PICKED.with(|p| *p.borrow_mut() = None);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(
        View::new(palette::WHITE, component(|| OverlayHost::wrap(column(vec![text("x").into_widget()]))))
            .into_widget(),
    );
    ui.layout(&mut env, win);
    overlay::set_window_size(400.0, 300.0);

    // The shell calls this after an unclaimed right-click.
    pebbles_widgets::global_menu::show(40.0, 40.0);
    frame(&mut ui, &mut env, win);
    assert!(overlay::is_open(), "an unclaimed right-click opens the global menu");
    overlay::hide_overlay();
    frame(&mut ui, &mut env, win);

    // Disabled → nothing opens.
    set_global_menu_enabled(false);
    pebbles_widgets::global_menu::show(40.0, 40.0);
    frame(&mut ui, &mut env, win);
    assert!(!overlay::is_open(), "disabled: no menu");
    set_global_menu_enabled(true);

    // Custom options replace the standard set; the custom item fires.
    set_global_menu(vec![
        menu_item("My action").on_select(|| PICKED.with(|p| *p.borrow_mut() = Some("action".into()))).into(),
    ]);
    pebbles_widgets::global_menu::show(40.0, 40.0);
    frame(&mut ui, &mut env, win);
    assert!(overlay::is_open(), "custom menu opens");

    // The panel opens at (40, 40); the first row spans y ≈ 44..76.
    let p = pebbles_foundation::Offset::new(60.0, 60.0);
    ui.dispatch_pointer_down(p);
    ui.dispatch_tap(p);
    ui.dispatch_pointer_up(p);
    frame(&mut ui, &mut env, win);
    assert_eq!(PICKED.with(|p| p.borrow().clone()), Some("action".into()));
    assert!(!overlay::is_open(), "picking closes the menu");
}

#[test]
fn standard_items_dispatch_to_the_focused_editor() {
    init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                OverlayHost::wrap(
                    column(vec![text_field().autofocus().width(200.0).into_widget()])
                        .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start),
                )
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, win);
    overlay::set_window_size(400.0, 300.0);
    frame(&mut ui, &mut env, win);

    // Type something, then Select All from the global menu selects it — proven
    // by typing over it (replaces everything).
    ui.dispatch_key(KeyInput::Insert("hello".to_string()));
    frame(&mut ui, &mut env, win);

    pebbles_widgets::global_menu::show(40.0, 40.0);
    frame(&mut ui, &mut env, win);

    // Panel at (40, 40): Cut 44..76, Copy 76..108, Paste 108..140, separator,
    // Select All ≈ 149..181.
    let p = pebbles_foundation::Offset::new(60.0, 165.0);
    ui.dispatch_pointer_down(p);
    ui.dispatch_tap(p);
    ui.dispatch_pointer_up(p);
    frame(&mut ui, &mut env, win);

    ui.dispatch_key(KeyInput::Insert("X".to_string()));
    frame(&mut ui, &mut env, win);
    let value = ui
        .render_tree()
        .semantics_tree()
        .iter()
        .find(|n| n.props.role == pebbles_render::SemanticsRole::TextInput)
        .and_then(|n| n.props.value.clone())
        .unwrap_or_default();
    assert_eq!(value, "X", "Select All from the global menu selected the editor text");
}

#[test]
fn block_context_menu_consumes_right_clicks() {
    init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                OverlayHost::wrap(
                    column(vec![block_context_menu(text("blocked")).into_widget()])
                        .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start),
                )
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, win);
    overlay::set_window_size(400.0, 300.0);
    frame(&mut ui, &mut env, win);

    // The blocker consumes the secondary tap — the shell would not open the
    // global menu.
    let handled = ui.dispatch_secondary_tap(pebbles_foundation::Offset::new(10.0, 10.0));
    assert!(handled, "a blocked area consumes the right-click");
    pebbles_widgets::global_menu::show(10.0, 10.0);
    frame(&mut ui, &mut env, win);
    assert!(overlay::is_open(), "calling show directly still works (the shell skips it)");
}

#[test]
fn styled_surface_paints() {
    init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(
        View::new(palette::WHITE, component(|| OverlayHost::wrap(column(vec![text("x").into_widget()]))))
            .into_widget(),
    );
    ui.layout(&mut env, win);
    overlay::set_window_size(400.0, 300.0);

    set_global_menu_style(style().background(palette::BLUE).radius_all(0.0));
    pebbles_widgets::global_menu::show(40.0, 40.0);
    frame(&mut ui, &mut env, win);
    assert!(overlay::is_open());
}

#[test]
fn defaults_disabled_but_opt_ins_work() {
    init();
    set_global_menu_enabled(false); // the new default

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(
        View::new(palette::WHITE, component(|| OverlayHost::wrap(column(vec![text("x").into_widget()]))))
            .into_widget(),
    );
    ui.layout(&mut env, win);
    overlay::set_window_size(400.0, 300.0);

    // The shell's fallback is a no-op while disabled…
    pebbles_widgets::global_menu::show(40.0, 40.0);
    frame(&mut ui, &mut env, win);
    assert!(!overlay::is_open(), "disabled: the fallback opens nothing");

    // …but explicit opens ignore the switch.
    pebbles_widgets::global_menu::show_here(40.0, 40.0);
    frame(&mut ui, &mut env, win);
    assert!(overlay::is_open(), "show_here opens regardless of the switch");
    overlay::hide_overlay();
    frame(&mut ui, &mut env, win);

    // global_menu_on attaches the default menu to a widget — works while off.
    let handled = ui.dispatch_secondary_tap(pebbles_foundation::Offset::new(10.0, 10.0));
    assert!(!handled, "plain text claims nothing");
    overlay::hide_overlay();
}

#[test]
fn widget_opt_ins_open_the_menu_while_disabled() {
    init();
    set_global_menu_enabled(false);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                OverlayHost::wrap(
                    column(vec![pebbles_widgets::global_menu_on(text("opt-in")).into_widget()])
                        .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start),
                )
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, win);
    overlay::set_window_size(400.0, 300.0);
    frame(&mut ui, &mut env, win);

    let p = pebbles_foundation::Offset::new(10.0, 10.0);
    let handled = ui.dispatch_secondary_tap(p);
    frame(&mut ui, &mut env, win);
    assert!(handled, "the opt-in widget claims the right-click");
    assert!(overlay::is_open(), "and opens the default menu while global is off");
}

#[test]
fn buttons_consume_right_clicks_until_opted_in() {
    init();
    set_global_menu_enabled(true);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 300.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                OverlayHost::wrap(
                    column(vec![
                        pebbles_widgets::button("plain").into_widget(),
                        pebbles_widgets::button("opted")
                            .context_menu(|| PICKED.with(|p| *p.borrow_mut() = Some("ctx".into())))
                            .into_widget(),
                    ])
                    .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start),
                )
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, win);
    overlay::set_window_size(400.0, 300.0);
    frame(&mut ui, &mut env, win);
    PICKED.with(|p| *p.borrow_mut() = None);

    // Plain button (y ≈ 0..36): consumes, nothing opens.
    let p = pebbles_foundation::Offset::new(30.0, 18.0);
    assert!(ui.dispatch_secondary_tap(p), "a button consumes right-clicks by default");
    pebbles_widgets::global_menu::show(40.0, 40.0); // shell would skip — sanity: still opens on demand
    frame(&mut ui, &mut env, win);
    overlay::hide_overlay();
    assert_eq!(PICKED.with(|p| p.borrow().clone()), None);

    // Opted-in button (y ≈ 36..72): fires the developer's handler.
    let q = pebbles_foundation::Offset::new(30.0, 54.0);
    assert!(ui.dispatch_secondary_tap(q));
    assert_eq!(PICKED.with(|p| p.borrow().clone()), Some("ctx".into()));
}
