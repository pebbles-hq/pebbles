//! Text editing polish (checklist 1.2): the caret blinks (~2 Hz) while the field
//! is focused and stays solid while composing; multiline Up/Down keep the caret's
//! column (parley's affinity-based vertical navigation), proven by inserting a
//! character after the move and asserting where it landed.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, KeyInput, Motion, Signal, Ui, animation, component, create_signal};
use pebbles_foundation::{Size, palette};
use pebbles_render::{RenderTextField, TextEnv};
use pebbles_widgets::{OverlayHost, View, column, text_area, text_field};

thread_local! {
    static VALUE: RefCell<Option<Signal<String>>> = const { RefCell::new(None) };
}

fn bound() -> Signal<String> {
    VALUE.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            *c = Some(create_signal(String::new()));
        }
        c.unwrap()
    })
}

fn up() -> KeyInput {
    KeyInput::Move { motion: Motion::Up, extend: false }
}

fn down() -> KeyInput {
    KeyInput::Move { motion: Motion::Down, extend: false }
}

fn caret_visible(ui: &Ui) -> bool {
    let tree = ui.render_tree();
    let rid = tree.find::<RenderTextField>().expect("a text field render object");
    tree.object_ref(rid).downcast_ref::<RenderTextField>().expect("RenderTextField").caret_visible
}

#[test]
fn caret_blinks_while_focused_and_is_solid_while_composing() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    animation::reset();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 120.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                OverlayHost::wrap(
                    column(vec![text_field().autofocus().width(240.0).into_widget()])
                        .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start),
                )
            }),
        )
        .into_widget(),
    );
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, win);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut env, &mut scene);
    };
    frame(&mut ui);
    assert!(caret_visible(&ui), "caret starts visible");

    // Half a period later the blink phase hides it; another half shows it again.
    animation::tick(0.3);
    frame(&mut ui);
    assert!(!caret_visible(&ui), "hidden in the second half of the blink cycle");
    animation::tick(0.55);
    frame(&mut ui);
    assert!(caret_visible(&ui), "visible again in the next cycle");

    // While composing, the caret never blinks.
    ui.dispatch_key(KeyInput::Preedit("に".to_string()));
    frame(&mut ui);
    animation::tick(0.8);
    frame(&mut ui);
    assert!(caret_visible(&ui), "solid while an IME composition is active");
}

#[test]
fn multiline_up_down_preserve_the_column() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = bound();
    bound().set(String::new());

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(400.0, 200.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                OverlayHost::wrap(
                    column(vec![text_area(4).bind(bound()).autofocus().width(300.0).into_widget()])
                        .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start),
                )
            }),
        )
        .into_widget(),
    );
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, win);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut env, &mut scene);
    };
    frame(&mut ui); // autofocus

    // "aaaaaa\n" + "aaaa" — caret at the end of the second (4-char) line.
    ui.dispatch_key(KeyInput::Insert("aaaaaa".to_string()));
    ui.dispatch_key(KeyInput::Enter);
    ui.dispatch_key(KeyInput::Insert("aaaa".to_string()));
    frame(&mut ui);
    assert_eq!(bound().peek(), "aaaaaa\naaaa");

    // Up: column memory must land the caret after the 4th 'a' of line 1 (same
    // glyphs → same x), NOT at the line's end.
    ui.dispatch_key(up());
    frame(&mut ui);
    ui.dispatch_key(KeyInput::Insert("X".to_string()));
    frame(&mut ui);
    assert_eq!(bound().peek(), "aaaaXaa\naaaa", "Up preserves the visual column");

    // Down: back to the second line at the same column — its end (offset 11).
    ui.dispatch_key(down());
    frame(&mut ui);
    ui.dispatch_key(KeyInput::Insert("Y".to_string()));
    frame(&mut ui);
    assert_eq!(bound().peek(), "aaaaXaa\naaaaY", "Down returns to the remembered column");
}
