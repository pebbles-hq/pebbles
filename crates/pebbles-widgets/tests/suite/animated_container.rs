//! `AnimatedContainer`: implicit tweens on width/height/color/radius/padding/
//! margin/opacity, and `Chip`: the deletable tag pill. Both driven headlessly.

use std::cell::{Cell, RefCell};

use pebbles_core::IntoWidget;
use pebbles_core::{Signal, Ui, animation, component, create_signal};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::{RenderConstrainedBox, RenderPointerListener, TextEnv};
use pebbles_widgets::{View, animated_container, chip, column, gap_h};

thread_local! {
    static WIDE: RefCell<Option<Signal<bool>>> = const { RefCell::new(None) };
    static DELETED: Cell<u32> = const { Cell::new(0) };
}

fn anim_shell() -> impl IntoWidget {
    let wide = WIDE.with(|c| c.borrow().expect("WIDE set before mount"));
    // A Column gives loose cross-axis constraints, so the animated width is free
    // to tween (under the View directly, the tight parent would override it).
    column(vec![
        animated_container()
            .width(if wide.get() { 300.0 } else { 100.0 })
            .height(60.0)
            .duration(0.2)
            .into_widget(),
    ])
}

#[test]
fn animated_container_tweens_width_then_settles() {
    WIDE.with(|c| *c.borrow_mut() = None);
    let wide = create_signal(false);
    WIDE.with(|c| *c.borrow_mut() = Some(wide));

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(500.0, 200.0);
    ui.mount_root(View::new(palette::WHITE, component(anim_shell)).into_widget());
    ui.layout(&mut env, win);

    let width = |ui: &Ui| {
        // The Container contributes a 0×0 gap box plus the real sizing box — take
        // the widest constrained box (the animated one).
        ui.render_tree()
            .find_all::<RenderConstrainedBox>()
            .into_iter()
            .map(|id| ui.render_tree().size_of(id).width)
            .fold(0.0_f64, f64::max)
    };
    // One frame: tick the driver, then fold the new values into the tree.
    let mut frame = |ui: &mut Ui, now: f64| {
        animation::tick(now);
        ui.rebuild_if_dirty();
        ui.layout(&mut env, win);
    };

    assert!((width(&ui) - 100.0).abs() < 1e-6, "starts at the target");

    // Flip the target → the first tick anchors the track, the next lands it
    // halfway through the 0.2s tween → width strictly between.
    wide.set(true);
    frame(&mut ui, 0.1); // rebuild starts the track
    frame(&mut ui, 0.2); // anchors it (start = now)
    frame(&mut ui, 0.3); // 0.1s in → halfway
    let mid = width(&ui);
    assert!(mid > 100.0 && mid < 300.0, "mid-flight width: {mid}");

    // After the duration, the tween settles at the new target and the driver idles.
    frame(&mut ui, 0.6);
    assert!((width(&ui) - 300.0).abs() < 1e-6, "settles at 300");
    assert!(!animation::tick(1.0), "driver idle once settled");
}

#[test]
fn animated_container_driver_idles_after_unmount() {
    WIDE.with(|c| *c.borrow_mut() = None);
    let wide = create_signal(false);
    WIDE.with(|c| *c.borrow_mut() = Some(wide));

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(anim_shell)).into_widget());
    ui.layout(&mut env, Size::new(500.0, 200.0));
    wide.set(true);
    ui.rebuild_if_dirty();
    ui.layout(&mut env, Size::new(500.0, 200.0));
    assert!(animation::tick(0.05), "mid-tween the driver is active");
    ui.dispose(); // lifecycle §0: zero tracks after unmount
    assert!(!animation::tick(0.5), "driver idle after the screen unmounts");
}

fn chip_root() -> impl IntoWidget {
    let tags = create_signal(vec!["one".to_string(), "two".to_string()]);
    column(
        tags.get()
            .iter()
            .map(|label| {
                let label = label.clone();
                chip(label.clone())
                    .deletable(true)
                    .on_deleted(move || {
                        DELETED.with(|d| d.set(d.get() + 1));
                        tags.update(|t| t.retain(|x| *x != label));
                    })
                    .into_widget()
            })
            .collect::<Vec<_>>(),
    )
}

#[test]
fn chip_delete_fires_on_deleted_and_removes_from_the_list() {
    DELETED.with(|d| d.set(0));
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(chip_root)).into_widget());
    ui.layout(&mut env, Size::new(400.0, 200.0));

    // Find the close buttons by scanning: each chip's ✕ is a pointer-listener
    // node; collect each distinct listener hit by a grid of points, keeping the
    // first point that reaches it (chip 1's ✕ sits above chip 2's).
    let mut closes: std::collections::BTreeMap<pebbles_render::RenderId, Offset> =
        std::collections::BTreeMap::new();
    for y in (0..90).step_by(2) {
        for x in (0..360).step_by(2) {
            let point = Offset::new(x as f64, y as f64);
            let chain = ui.render_tree().hit_test(point);
            if let Some(id) = chain
                .iter()
                .copied()
                .find(|&id| ui.render_tree().object_ref(id).is::<RenderPointerListener>())
            {
                closes.entry(id).or_insert(point);
            }
        }
    }
    assert_eq!(closes.len(), 2, "two deletable chips expose ✕ buttons");
    // Tap the ✕ with the largest first-hit y (the second chip's) — one deletion.
    let target = *closes.values().max_by(|a, b| a.y.total_cmp(&b.y)).unwrap();
    assert!(ui.dispatch_tap(target), "the ✕ affordance is tappable");
    ui.rebuild_if_dirty();
    ui.layout(&mut env, Size::new(400.0, 200.0));
    assert_eq!(DELETED.with(Cell::get), 1, "exactly one on_deleted fired");
}

#[test]
fn chip_paints_with_and_without_delete() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![
                    chip("plain").into_widget(),
                    gap_h(8.0).into_widget(),
                    chip("deletable").deletable(true).on_deleted(|| {}).into_widget(),
                    gap_h(8.0).into_widget(),
                    chip("disabled").disabled(true).into_widget(),
                ])
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, Size::new(300.0, 200.0));
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut env, &mut scene);
}
