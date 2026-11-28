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

    let sel = || SEL.with(|s| *s.borrow());

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
    assert_eq!(SEL.with(|s| *s.borrow()), 2, "precondition: keyboard moved to tab 2");

    // Tab 0's strip button: first cell of the row (y ≈ 8..40), center ≈ (29, 17).
    let tap = |ui: &mut Ui, p: Offset| {
        ui.dispatch_pointer_down(p);
        ui.dispatch_tap(p);
        ui.dispatch_pointer_up(p);
    };
    tap(&mut ui, Offset::new(29.0, 17.0));
    frame(&mut ui);
    assert_eq!(SEL.with(|s| *s.borrow()), 0, "tapping tab 0 reports selection 0");
}

fn solid_on_violet() -> impl IntoWidget {
    tabs(0)
        .variant(TabsVariant::Solid)
        .tab("Chat", body("a"), || {})
        .tab("Files", body("b"), || {})
        .style(pebbles_widgets::style().background(palette::violet::S600))
}

/// Regression: unselected labels on a caller-coloured Solid strip used the theme's muted
/// grey (unreadable on violet). Like Flutter's TabBar they now take the strip's readable
/// on-colour (white on a dark strip) at 70 %.
#[test]
fn unselected_label_reads_on_a_coloured_strip() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(solid_on_violet)).into_widget());
    ui.layout(&mut env, Size::new(400.0, 300.0));
    let tree = ui.render_tree();
    let files = tree
        .find_all::<pebbles_render::RenderParagraph>()
        .into_iter()
        .map(|id| tree.object_ref(id).downcast_ref::<pebbles_render::RenderParagraph>().unwrap())
        .find(|p| p.text == "Files")
        .expect("the Files label");
    let [r, g, b, a] = files.style.color.components;
    assert!(r > 0.95 && g > 0.95 && b > 0.95, "white on violet, got {:?}", files.style.color);
    assert!((a - 0.7).abs() < 0.01, "70 % like Flutter's unselected label, got {a}");
}

// --- the one tab design: a 2 px accent underline on the selected tab's bottom edge ---

fn strip_view() -> impl IntoWidget {
    let sel = create_signal(1usize);
    pebbles_widgets::tab_strip(sel.get())
        .item("Sources", Some(3), move || sel.set(0))
        .item("Fields", None, move || sel.set(1))
        .item("Filters", None, move || sel.set(2))
        .tab_disabled(2)
}

#[test]
fn strip_indicator_is_a_two_px_underline_under_the_whole_selected_tab() {
    let mut h = pebbles_testing::Harness::new().window(500.0, 120.0);
    h.mount(strip_view);
    h.draw();
    let tree = h.semantics_tree();
    let tab =
        |l: &str| tree.iter().find(|n| n.props.label == l).unwrap_or_else(|| panic!("no tab {l}")).bounds;
    let ind: Vec<_> = tree.iter().filter(|n| n.props.label == pebbles_widgets::TAB_INDICATOR_LABEL).collect();
    assert_eq!(ind.len(), 1, "exactly one indicator: on the selected tab");
    let (i, t) = (ind[0].bounds, tab("Fields"));
    assert_eq!(i.y1, t.y1, "on the tab's BOTTOM edge");
    assert_eq!(i.y1 - i.y0, pebbles_widgets::TAB_INDICATOR_H, "2 px tall");
    assert_eq!((i.x0, i.x1), (t.x0, t.x1), "spanning the tab's full width, not a left bar");
    assert!(i.x1 - i.x0 > 4.0 * (i.y1 - i.y0), "wide, never a left-edge bar");

    // Tapping another tab moves the indicator; the disabled tab can't take it.
    let s = tab("Sources");
    h.click(Offset::new((s.x0 + s.x1) / 2.0, (s.y0 + s.y1) / 2.0));
    h.frames(2);
    let tree = h.semantics_tree();
    let ind = tree.iter().find(|n| n.props.label == pebbles_widgets::TAB_INDICATOR_LABEL).unwrap().bounds;
    let s = tree.iter().find(|n| n.props.label == "Sources").unwrap().bounds;
    assert_eq!((ind.x0, ind.x1, ind.y1), (s.x0, s.x1, s.y1), "indicator followed the selection");
    let d = tree.iter().find(|n| n.props.label == "Filters").unwrap();
    assert!(d.props.disabled, "disabled tab is announced disabled");
    let f = d.bounds;
    h.click(Offset::new((f.x0 + f.x1) / 2.0, (f.y0 + f.y1) / 2.0));
    h.frames(2);
    let ind = h
        .semantics_tree()
        .into_iter()
        .find(|n| n.props.label == pebbles_widgets::TAB_INDICATOR_LABEL)
        .unwrap()
        .bounds;
    assert_eq!(ind.x0, s.x0, "a disabled tab ignores clicks");
}

#[test]
fn underline_tabs_show_hover_feedback_and_keep_their_size() {
    let mut h = pebbles_testing::Harness::new().window(500.0, 120.0);
    h.mount(strip_view);
    h.draw();
    let before = h.semantics_tree().into_iter().find(|n| n.props.label == "Fields").unwrap().bounds;
    h.hover(Offset::new((before.x0 + before.x1) / 2.0, (before.y0 + before.y1) / 2.0));
    h.frames(6);
    let after = h.semantics_tree().into_iter().find(|n| n.props.label == "Fields").unwrap().bounds;
    assert_eq!(before, after, "hover must not move or resize the tab");
}
