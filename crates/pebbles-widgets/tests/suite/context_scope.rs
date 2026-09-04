//! Render-time context: `theme_override` (scoped theme) and `focus_scope` (the
//! Tab focus trap). Driven headlessly through a real `Ui`.

use std::cell::RefCell;

use pebbles_core::focus;
use pebbles_core::{IntoWidget, Ui, component, component_props};
use pebbles_foundation::{Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{BadgeVariant, Theme, View, badge, button, column, focus_scope, row, theme_override};

thread_local! {
    static SEEN_DARK: RefCell<Vec<bool>> = const { RefCell::new(Vec::new()) };
}

#[derive(Clone)]
struct ProbeProps {
    label: &'static str,
}

/// A real function component: its body runs at RENDER time, so it reads the
/// theme exactly like any catalog component would.
fn probe(p: &ProbeProps) -> pebbles_core::Element {
    let dark = pebbles_widgets::theme().dark;
    SEEN_DARK.with(|c| c.borrow_mut().push(dark));
    badge(p.label).variant(BadgeVariant::Secondary).into_widget()
}

#[test]
fn theme_override_scopes_to_exactly_one_subtree() {
    SEEN_DARK.with(|c| c.borrow_mut().clear());
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![
                    component_props(probe, ProbeProps { label: "global" }).into_widget(),
                    theme_override(
                        Theme::dark(),
                        column(vec![component_props(probe, ProbeProps { label: "inner" }).into_widget()]),
                    )
                    .into_widget(),
                    component_props(probe, ProbeProps { label: "outside" }).into_widget(),
                ])
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, Size::new(400.0, 300.0));

    let seen = SEEN_DARK.with(|c| c.borrow().clone());
    assert_eq!(seen.len(), 3, "one probe per badge");
    assert!(!seen[0], "before the override sees the global (light) theme");
    assert!(seen[1], "inside the override sees dark");
    assert!(!seen[2], "a sibling after the override sees the global theme again");
}

#[test]
fn theme_override_nested_row_paints() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![
                    badge("a").variant(BadgeVariant::Secondary).into_widget(),
                    row(vec![theme_override(
                        Theme::dark(),
                        badge("b").variant(BadgeVariant::Secondary),
                    )
                    .into_widget()])
                    .into_widget(),
                ])
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, Size::new(300.0, 200.0));
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut env, &mut scene);
}

#[test]
fn focus_scope_traps_tab_cycling_within_its_subtree() {
    focus::init();
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![
                    button("Out 1").into_widget(),
                    button("Out 2").into_widget(),
                    focus_scope(column(vec![
                        button("In A").into_widget(),
                        button("In B").into_widget(),
                        button("In C").into_widget(),
                    ]))
                    .into_widget(),
                ])
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, Size::new(400.0, 300.0));

    let nodes = focus::registered_nodes(0);
    assert_eq!(nodes.len(), 5, "two outside + three inside");
    let (out1, _out2, in_a, in_b, in_c) = (nodes[0], nodes[1], nodes[2], nodes[3], nodes[4]);

    // Jump into the scope the way a dialog would (its first field autofocuses):
    // set focus on In C (the last registered inner node).
    focus::set_focus(Some(in_c));

    // Tab forward from In C wraps to In A (stays in scope)…
    assert!(focus::focus_move(0, true));
    assert_eq!(focus::focus_signal().peek(), Some(in_a), "wraps within the scope");
    assert!(focus::focus_move(0, true));
    assert_eq!(focus::focus_signal().peek(), Some(in_b), "In B stays in scope");
    assert!(focus::focus_move(0, true));
    assert_eq!(focus::focus_signal().peek(), Some(in_c), "In C again — never Out 1/2");

    // Backward too: In C → In B.
    assert!(focus::focus_move(0, false));
    assert_eq!(focus::focus_signal().peek(), Some(in_b));

    // Tab from nothing focuses the first ROOT-scope node (Out 1) — scoped nodes
    // are excluded from the root cycle until focus enters the scope.
    focus::set_focus(None);
    assert!(focus::focus_move(0, true));
    assert_eq!(focus::focus_signal().peek(), Some(out1), "root cycle skips scoped nodes");
    focus::set_focus(None);
}
