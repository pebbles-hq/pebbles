//! WidgetSpan (03 T50): inline widgets inside a rich paragraph — laid out as placeholders with
//! the text, wrapped like words, hit-tested before the text and present in semantics.

use std::cell::Cell;
use std::rc::Rc;

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::{RenderId, RenderTree, SemanticsRole, TextEnv};
use pebbles_widgets::{View, button, column, container, gesture_detector, span, text_rich, widget_span};

fn mount(root: fn() -> pebbles_core::AnyWidget, width: f64) -> (Ui, TextEnv) {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, Size::new(width, 400.0));
    (ui, env)
}

fn find(tree: &RenderTree, name: &str) -> Vec<RenderId> {
    let mut out = Vec::new();
    let mut stack: Vec<RenderId> = tree.root.into_iter().collect();
    while let Some(id) = stack.pop() {
        if tree.debug_name(id) == name {
            out.push(id);
        }
        stack.extend(tree.children_of(id).iter().rev().copied());
    }
    out
}

/// Window-space origin of `target` (offsets summed down the path from the root).
fn origin(tree: &RenderTree, target: RenderId) -> Offset {
    fn walk(tree: &RenderTree, id: RenderId, at: Offset, target: RenderId) -> Option<Offset> {
        let here = at + tree.offset_of(id);
        if id == target {
            return Some(here);
        }
        tree.children_of(id).iter().find_map(|&c| walk(tree, c, here, target))
    }
    tree.root.and_then(|r| walk(tree, r, Offset::new(0.0, 0.0), target)).expect("in the tree")
}

fn inline_box() -> pebbles_core::AnyWidget {
    column(vec![
        text_rich(vec![
            span("ab"),
            widget_span(|| container().width(20.0).height(10.0).into_widget()),
            span("cd"),
        ])
        .into_widget(),
    ])
    .into_widget()
}

#[test]
fn inline_icon_sits_on_baseline() {
    let (ui, _env) = mount(inline_box, 400.0);
    let tree = ui.render_tree();
    let para = find(tree, "RenderParagraph")[0];
    let child = tree.children_of(para)[0];
    let size = tree.size_of(child);
    assert_eq!((size.width, size.height), (20.0, 10.0));
    // No baseline of its own: the box's bottom sits on the text baseline (Inter 16 → 15.42).
    let bottom = tree.offset_of(child).y + size.height;
    assert!((bottom - 15.4203).abs() < 0.01, "bottom {bottom}");
    assert!(tree.offset_of(child).x > 0.0, "after \"ab\"");
}

fn wrapping() -> pebbles_core::AnyWidget {
    column(vec![
        text_rich(vec![
            span("aaaa bbbb "),
            widget_span(|| container().width(60.0).height(10.0).into_widget()),
            span(" cccc"),
        ])
        .into_widget(),
    ])
    .into_widget()
}

#[test]
fn inline_child_wraps_with_text() {
    let (ui, _env) = mount(wrapping, 90.0);
    let tree = ui.render_tree();
    let para = find(tree, "RenderParagraph")[0];
    let child = tree.children_of(para)[0];
    let at = tree.offset_of(child);
    assert!(at.y > 10.0, "the box moved to a later line: {at:?}");
    assert!(at.x < 1.0, "and starts it: {at:?}");
}

thread_local! {
    static TAPS: Rc<Cell<u32>> = Rc::new(Cell::new(0));
}

fn tappable() -> pebbles_core::AnyWidget {
    let taps = TAPS.with(Rc::clone);
    column(vec![
        text_rich(vec![
            span("ab"),
            widget_span(move || {
                let taps = taps.clone();
                gesture_detector(container().width(30.0).height(14.0))
                    .on_tap(pebbles_core::action_event(move |_| taps.set(taps.get() + 1)))
                    .into_widget()
            }),
            span("cd"),
        ])
        .into_widget(),
    ])
    .into_widget()
}

#[test]
fn tap_on_inline_child_hits_child() {
    let (mut ui, _env) = mount(tappable, 400.0);
    let tree = ui.render_tree();
    let para = find(tree, "RenderParagraph")[0];
    let child = tree.children_of(para)[0];
    let at = origin(tree, child) + Offset::new(15.0, 7.0);
    TAPS.with(|t| t.set(0));
    ui.dispatch_pointer_down(at);
    ui.dispatch_tap(at);
    ui.dispatch_pointer_up(at);
    assert_eq!(TAPS.with(|t| t.get()), 1, "the inline child received the tap");
}

fn with_button() -> pebbles_core::AnyWidget {
    column(vec![
        text_rich(vec![span("Press "), widget_span(|| button("Go").into_widget()), span(" to continue")])
            .into_widget(),
    ])
    .into_widget()
}

#[test]
fn semantics_order_includes_inline_child() {
    let (ui, _env) = mount(with_button, 400.0);
    let tree = ui.render_tree().semantics_tree();
    let btn =
        tree.iter().find(|n| n.props.role == SemanticsRole::Button).expect("the inline button has a node");
    assert_eq!(btn.props.label, "Go");
    assert!(btn.bounds.x0 > 0.0, "placed after \"Press \": {:?}", btn.bounds);
}
