//! Repaint boundaries (M30 AC1, Flutter `RepaintBoundary`): a clean boundary re-appends the
//! SAME retained fragment (`Arc` op list) frame after frame, and anything that changes what
//! it paints — a signal it reads, hover, focus, a caret blink — re-records it.
//!
//! The staleness oracle: after every frame the scene must equal (op for op, fragments
//! compared by content) the scene painted with every layer invalidated and re-recorded.

use pebbles_core::{AnyWidget, IntoWidget, Signal, Ui, animation, component, create_signal};
use pebbles_foundation::{Color, Offset, Size, palette};
use pebbles_render::paint::display_list::Op;
use pebbles_render::{RenderBoundary, RenderId, Scene, TextEnv};
use pebbles_widgets::{
    OverlayHost, SizedBox, View, colored_box, column, pressable, repaint_boundary, text, text_field,
};

const WIN: Size = Size::new(400.0, 300.0);

fn frame(ui: &mut Ui, env: &mut TextEnv) -> Scene {
    ui.rebuild_if_dirty();
    ui.layout(env, WIN);
    let mut scene = Scene::new();
    ui.paint(env, &mut scene);
    scene
}

/// The scene re-painted from scratch (every retained layer dropped) must equal `scene`.
/// Returns the fresh scene: keep it alive so the next re-record of any layer it shares
/// allocates a new list (a changed address then proves a re-record).
#[must_use]
fn assert_not_stale(ui: &mut Ui, env: &mut TextEnv, scene: &Scene, what: &str) -> Scene {
    ui.invalidate_layout();
    let fresh = frame(ui, env);
    assert_eq!(format!("{:?}", scene.ops()), format!("{:?}", fresh.ops()), "stale repaint boundary: {what}");
    fresh
}

/// Every explicit `RenderBoundary`, in mount order, with its retained op-list address.
fn boundary_layers(ui: &Ui) -> Vec<(RenderId, Option<usize>)> {
    let tree = ui.render_tree();
    tree.find_all::<RenderBoundary>().into_iter().map(|id| (id, tree.layer_ptr(id))).collect()
}

/// Every fragment op-list address reachable from `ops`.
fn fragment_ptrs(ops: &[Op], out: &mut Vec<usize>) {
    for op in ops {
        if let Op::Fragment { ops, .. } = op {
            out.push(ops.as_ptr() as usize);
            fragment_ptrs(ops, out);
        }
    }
}

fn card(c: Color) -> impl IntoWidget {
    colored_box(c, SizedBox::new(Some(120.0), Some(40.0), None))
}

thread_local! {
    static COUNT: std::cell::Cell<Option<Signal<i32>>> = const { std::cell::Cell::new(None) };
}

fn count() -> Signal<i32> {
    COUNT.with(|c| match c.get() {
        Some(s) => s,
        None => {
            let s = create_signal(0);
            c.set(Some(s));
            s
        }
    })
}

#[test]
fn clean_boundary_keeps_its_fragment_across_frames() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            column(vec![
                repaint_boundary(card(Color::from_rgb8(200, 0, 0))).into_widget(),
                repaint_boundary(text("static")).into_widget(),
            ]),
        )
        .into_widget(),
    );
    let first = frame(&mut ui, &mut env);
    let layers = boundary_layers(&ui);
    assert_eq!(layers.len(), 2);
    assert!(layers.iter().all(|(_, p)| p.is_some()), "both boundaries recorded a layer");
    let second = frame(&mut ui, &mut env);
    assert_eq!(boundary_layers(&ui), layers, "nothing changed: the same retained layers");
    let (mut a, mut b) = (Vec::new(), Vec::new());
    fragment_ptrs(first.ops(), &mut a);
    fragment_ptrs(second.ops(), &mut b);
    assert!(!b.is_empty() && a == b, "the scene carries the same fragments: {a:?} vs {b:?}");
    let _fresh = assert_not_stale(&mut ui, &mut env, &second, "idle frame");
}

#[test]
fn boundary_repaints_when_a_signal_it_reads_changes() {
    let mut ui = Ui::new();
    ui.make_current();
    let mut env = TextEnv::new();
    let n = count();
    n.set(0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            column(vec![
                repaint_boundary(component(move || text(format!("count {}", count().get())))).into_widget(),
                repaint_boundary(card(Color::from_rgb8(0, 0, 200))).into_widget(),
            ]),
        )
        .into_widget(),
    );
    // Keep each frame alive (the renderer does): a layer whose list is still shared is
    // re-recorded into a NEW list, so a changed address proves a re-record.
    let _prev = frame(&mut ui, &mut env);
    let before = boundary_layers(&ui);
    n.set(1);
    let scene = frame(&mut ui, &mut env);
    let after = boundary_layers(&ui);
    assert_ne!(after[0].1, before[0].1, "the counter's boundary re-recorded");
    assert_eq!(after[1], before[1], "the sibling card kept its fragment");
    let _fresh = assert_not_stale(&mut ui, &mut env, &scene, "counter increment");
}

thread_local! {
    static HOVERED: std::cell::Cell<Option<Signal<bool>>> = const { std::cell::Cell::new(None) };
}

fn hovered() -> Signal<bool> {
    HOVERED.with(|c| match c.get() {
        Some(s) => s,
        None => {
            let s = create_signal(false);
            c.set(Some(s));
            s
        }
    })
}

fn hover_card() -> AnyWidget {
    let c = if hovered().get() { Color::from_rgb8(250, 200, 0) } else { Color::from_rgb8(120, 120, 120) };
    pressable(card(c)).on_hover(|h| hovered().set(h)).into_widget()
}

#[test]
fn hover_invalidates_the_hovered_boundary_only() {
    let mut ui = Ui::new();
    ui.make_current();
    let mut env = TextEnv::new();
    hovered().set(false);
    ui.mount_root(
        View::new(
            palette::WHITE,
            column(vec![
                repaint_boundary(component(hover_card)).into_widget(),
                repaint_boundary(card(Color::from_rgb8(0, 150, 0))).into_widget(),
            ]),
        )
        .into_widget(),
    );
    let idle = frame(&mut ui, &mut env);
    let before = boundary_layers(&ui);
    assert!(ui.dispatch_hover(Offset::new(200.0, 10.0)), "the centered card takes the hover");
    let hovered_scene = frame(&mut ui, &mut env);
    let after = boundary_layers(&ui);
    let _fresh = assert_not_stale(&mut ui, &mut env, &hovered_scene, "hover");
    assert_ne!(format!("{:?}", idle.ops()), format!("{:?}", hovered_scene.ops()), "hover changed the paint");
    assert_ne!(after[0].1, before[0].1, "the hovered card's boundary re-recorded");
    assert_eq!(after[1], before[1], "the sibling card kept its fragment");
}

#[test]
fn focus_and_caret_blink_invalidate_the_field_only() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    animation::reset();
    let mut ui = Ui::new();
    ui.make_current();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            OverlayHost::wrap(column(vec![
                text_field().autofocus().width(240.0).into_widget(),
                repaint_boundary(card(Color::from_rgb8(0, 150, 150))).into_widget(),
            ])),
        )
        .into_widget(),
    );
    let focused = frame(&mut ui, &mut env);
    let _f0 = assert_not_stale(&mut ui, &mut env, &focused, "autofocus");
    // Half a blink period later the caret hides: only the field re-records.
    let card_layer = boundary_layers(&ui);
    animation::tick(0.3);
    let blinked = frame(&mut ui, &mut env);
    assert_eq!(boundary_layers(&ui), card_layer, "the card kept its fragment through the blink");
    assert_ne!(format!("{:?}", focused.ops()), format!("{:?}", blinked.ops()), "the caret blinked");
    let _f1 = assert_not_stale(&mut ui, &mut env, &blinked, "caret blink");
    // And back.
    animation::tick(0.55);
    let shown = frame(&mut ui, &mut env);
    let _f2 = assert_not_stale(&mut ui, &mut env, &shown, "caret blink back");
    assert_ne!(format!("{:?}", blinked.ops()), format!("{:?}", shown.ops()), "the caret is back");
    // Blur: the focus ring and caret go away; the card still keeps its fragment.
    let card_layer = boundary_layers(&ui);
    pebbles_core::focus::set_focus(None);
    let blurred = frame(&mut ui, &mut env);
    assert_eq!(boundary_layers(&ui), card_layer, "the card kept its fragment through the blur");
    assert_ne!(format!("{:?}", shown.ops()), format!("{:?}", blurred.ops()), "blur changed the paint");
    let _f3 = assert_not_stale(&mut ui, &mut env, &blurred, "blur");
}
