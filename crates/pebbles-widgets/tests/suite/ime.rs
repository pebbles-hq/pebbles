//! IME composition: an `Ime::Preedit` shows underlined, uncommitted text and does
//! NOT change the field value; an `Ime::Commit` (routed as `KeyInput::Insert`) commits
//! it and clears the composition. Exercises the full focus → dispatch_key → editor
//! path headlessly (the CJK text-entry story).

use std::cell::RefCell;

use pebbles_core::{IntoWidget, KeyInput, Signal, Ui, component, create_signal};
use pebbles_foundation::{Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{OverlayHost, View, column, text_field};

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

fn root() -> impl IntoWidget {
    OverlayHost::wrap(column(vec![
        text_field().bind(bound()).autofocus().width(240.0).into_widget(),
    ]))
}

#[test]
fn ime_preedit_is_uncommitted_then_commit_inserts() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let _ = bound(); // create the bound signal at app scope, before mount

    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    let window = Size::new(400.0, 200.0);
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut text, window);

    let frame = |ui: &mut Ui, text: &mut TextEnv| {
        ui.rebuild_if_dirty();
        ui.layout(text, window);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut scene); // a real paint — composing must not panic
    };
    frame(&mut ui, &mut text); // autofocus() focuses the field on mount

    // Type a real character first so we also prove the preedit splices at the caret.
    assert!(ui.dispatch_key(KeyInput::Insert("a".to_string())), "field is focused");
    frame(&mut ui, &mut text);
    assert_eq!(bound().peek(), "a");

    // Begin composition — preedit text is shown but NOT committed to the value.
    assert!(ui.dispatch_key(KeyInput::Preedit("に".to_string())), "preedit routes to the field");
    frame(&mut ui, &mut text);
    assert_eq!(bound().peek(), "a", "preedit does not change the field value");

    // Update the composition (still uncommitted).
    ui.dispatch_key(KeyInput::Preedit("にほ".to_string()));
    frame(&mut ui, &mut text);
    assert_eq!(bound().peek(), "a", "an updated preedit is still uncommitted");

    // Commit (Ime::Commit → Insert) — the composed text lands and composition clears.
    ui.dispatch_key(KeyInput::Insert("日本".to_string()));
    frame(&mut ui, &mut text);
    assert_eq!(bound().peek(), "a日本", "commit inserts the composed text at the caret");

    // A subsequent keystroke behaves normally (preedit is gone).
    ui.dispatch_key(KeyInput::Insert("!".to_string()));
    frame(&mut ui, &mut text);
    assert_eq!(bound().peek(), "a日本!");
}
