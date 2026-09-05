//! Widget-catalog forms batch: Field (3.6) labeled-control composite and ToggleGroup
//! (3.5) single/multi-select. Field is verified by painting; ToggleGroup's selection
//! logic is verified by driving taps through a real Ui.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, KeyInput, Signal, Ui, component, create_signal};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{OverlayHost, View, column, field, text_field, toggle_group_labels};

#[test]
fn field_paints_label_control_and_error() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    fn root() -> impl IntoWidget {
        // error_opt(Some) must replace the description with the red message.
        field(text_field().width(200.0))
            .label("Email")
            .description("We never share it.")
            .error_opt(Some("Required"))
    }

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, Size::new(300.0, 200.0));
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut env, &mut scene); // must lay out + paint the label/control/error stack, no panic
}

thread_local! {
    static SEL: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

fn group_root() -> impl IntoWidget {
    // Single-select group; report the picked indices.
    OverlayHost::wrap(
        toggle_group_labels(["Left", "Center", "Right"])
            .value(0)
            .on_changed(|sel| SEL.with(|c| *c.borrow_mut() = sel.to_vec())),
    )
}

#[test]
fn toggle_group_single_select_picks_one() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    SEL.with(|c| c.borrow_mut().clear());

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let window = Size::new(400.0, 120.0);
    ui.mount_root(View::new(palette::WHITE, component(group_root)).into_widget());
    ui.layout(&mut env, window);
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, window);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut env, &mut scene);
    };
    frame(&mut ui);

    // The three cells sit left-to-right as a JOINED strip (no gaps); each is ~52px
    // wide, so the middle cell spans x ≈ 52..104.
    let mid = Offset::new(78.0, 18.0);
    ui.dispatch_pointer_down(mid);
    ui.dispatch_tap(mid);
    ui.dispatch_pointer_up(mid);
    frame(&mut ui);

    assert_eq!(SEL.with(|c| c.borrow().clone()), vec![1], "tapping the middle cell selects index 1");

    // Joined-strip guard (C4): the point that used to be the 6px gap between
    // cells now lands on the second cell — no gaps in the joined strip.
    SEL.with(|c| c.borrow_mut().clear());
    let seam = Offset::new(55.0, 18.0);
    ui.dispatch_pointer_down(seam);
    ui.dispatch_tap(seam);
    ui.dispatch_pointer_up(seam);
    frame(&mut ui);
    assert_eq!(
        SEL.with(|c| c.borrow().clone()),
        vec![1],
        "the former gap position is cell 2 — the strip is joined with no gaps"
    );
}

thread_local! {
    static RO: RefCell<Option<Signal<String>>> = const { RefCell::new(None) };
}
fn ro_bound() -> Signal<String> {
    RO.with(|c| {
        let mut c = c.borrow_mut();
        if c.is_none() {
            *c = Some(create_signal("locked".to_string()));
        }
        c.unwrap()
    })
}
fn ro_root() -> impl IntoWidget {
    OverlayHost::wrap(column(vec![
        text_field().bind(ro_bound()).read_only(true).autofocus().width(220.0).into_widget(),
    ]))
}

#[test]
fn read_only_field_receives_keys_but_ignores_edits() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    RO.with(|c| *c.borrow_mut() = None);
    let _ = ro_bound();

    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    let window = Size::new(300.0, 150.0);
    ui.mount_root(View::new(palette::WHITE, component(ro_root)).into_widget());
    ui.layout(&mut text, window);
    ui.rebuild_if_dirty();
    ui.layout(&mut text, window); // autofocus() focuses the field

    // The read-only field IS focused (keys are delivered), but every mutation is
    // dropped — typing and deletion leave the value untouched.
    assert!(ui.dispatch_key(KeyInput::Insert("x".to_string())), "read-only field is focusable");
    assert!(ui.dispatch_key(KeyInput::Backspace), "and still receives keys");
    ui.rebuild_if_dirty();
    ui.layout(&mut text, window);
    assert_eq!(ro_bound().peek(), "locked", "typing/deletion are ignored in read-only mode");
}
