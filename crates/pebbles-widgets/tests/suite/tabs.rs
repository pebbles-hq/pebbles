//! [`Tabs`]: the strip is keyboard-navigable — Left/Right cycle to the next enabled
//! tab (wrapping, skipping disabled ones) — taps switch tabs, and both variants
//! (underline, pills) render + paint.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, KeyInput, Motion, Ui, component, create_signal};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{TabsVariant, View, body, column, tabs};

thread_local! {
    static SEL: RefCell<usize> = const { RefCell::new(0) };
}

/// A controlled tab strip with tab 1 ("Two") disabled: keyboard navigation must
/// skip it and wrap around both ends.
fn tabs_view() -> impl IntoWidget {
    let sel = create_signal(0usize);
    tabs(sel.get())
        .autofocus()
        .tab("One", body("one"), {
            move || {
                SEL.with(|s| *s.borrow_mut() = 0);
                sel.set(0);
            }
        })
        .tab("Two", body("two"), {
            move || {
                SEL.with(|s| *s.borrow_mut() = 1);
                sel.set(1);
            }
        })
        .tab_disabled(1)
        .tab("Three", body("three"), {
            move || {
                SEL.with(|s| *s.borrow_mut() = 2);
                sel.set(2);
            }
        })
}

fn right() -> KeyInput {
    KeyInput::Move { motion: Motion::Right, extend: false }
}

fn left() -> KeyInput {
    KeyInput::Move { motion: Motion::Left, extend: false }
}

#[test]
fn keyboard_cycles_enabled_tabs_and_wraps() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    SEL.with(|s| *s.borrow_mut() = 0);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(500.0, 300.0);
    ui.mount_root(View::new(palette::WHITE, component(tabs_view)).into_widget());
    ui.layout(&mut env, win);
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, win);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut env, &mut scene);
    };
    frame(&mut ui);

    let sel = || SEL.with(|s| s.borrow().clone());

    // Right from 0 skips the disabled tab 1 and lands on 2.
    ui.dispatch_key(right());
    frame(&mut ui);
    assert_eq!(sel(), 2, "Right must skip the disabled tab");

    // Right from the last enabled tab wraps to the first.
    ui.dispatch_key(right());
    frame(&mut ui);
    assert_eq!(sel(), 0, "Right at the end must wrap to the first enabled tab");

    // Left from the first wraps to the last enabled tab (1 is disabled).
    ui.dispatch_key(left());
    frame(&mut ui);
    assert_eq!(sel(), 2, "Left at the start must wrap to the last enabled tab");
}

#[test]
fn taps_switch_and_both_variants_paint() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    SEL.with(|s| *s.borrow_mut() = 0);

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(500.0, 300.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            column(vec![
                component(tabs_view).into_widget(),
                // A pills strip with a disabled tab: must lay out and paint.
                tabs(0usize)
                    .variant(TabsVariant::Pills)
                    .tab("A", body("a"), || {})
                    .tab("B", body("b"), || {})
                    .tab_disabled(1)
                    .into_widget(),
            ])
            .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Stretch),
        )
        .into_widget(),
    );
    ui.layout(&mut env, win);
    let mut frame = |ui: &mut Ui| {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, win);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut env, &mut scene);
    };
    frame(&mut ui);

    // Move to tab 2 by keyboard first, then prove a TAP on tab 0 switches back
    // (a silently dead tap would leave the selection at 2 and fail here).
    ui.dispatch_key(right());
    frame(&mut ui);
    assert_eq!(SEL.with(|s| s.borrow().clone()), 2, "precondition: keyboard moved to tab 2");

    // Tab 0's strip button: first cell of the row (y ≈ 8..40), center ≈ (29, 17).
    let tap = |ui: &mut Ui, p: Offset| {
        ui.dispatch_pointer_down(p);
        ui.dispatch_tap(p);
        ui.dispatch_pointer_up(p);
    };
    tap(&mut ui, Offset::new(29.0, 17.0));
    frame(&mut ui);
    assert_eq!(SEL.with(|s| s.borrow().clone()), 0, "tapping tab 0 reports selection 0");
}
