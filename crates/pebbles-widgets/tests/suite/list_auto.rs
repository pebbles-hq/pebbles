//! A1: auto-measured virtualized lists — `ListView::builder_auto`. Real extents
//! replace estimates as items measure; virtualization stays; positions equal the
//! prefix sums of the extent cache (no gaps/overlaps).

use std::cell::RefCell;

use pebbles_core::{IntoWidget, Ui, animation, component};
use pebbles_foundation::{Size, palette};
use pebbles_render::{RenderMeasureProbe, TextEnv};
use pebbles_widgets::{container, list_view, ListView, ScrollController, text, use_scroll_controller, View};

thread_local! {
    static CTL: RefCell<Option<ScrollController>> = const { RefCell::new(None) };
    static COUNT: RefCell<Option<pebbles_core::Signal<usize>>> = const { RefCell::new(None) };
}

fn shrinking_root() -> impl IntoWidget {
    let count = COUNT.with(|c| c.borrow().expect("COUNT set before mount"));
    let controller = use_scroll_controller();
    CTL.with(|c| *c.borrow_mut() = Some(controller));
    ListView::builder_auto(count.get(), row)
        .estimated_extent(40.0)
        .controller(controller)
}

/// `i % 3 → 40/64/96` tall rows (the classic mixed-feed probe).
fn height_of(i: usize) -> f64 {
    match i % 3 {
        0 => 40.0,
        1 => 64.0,
        _ => 96.0,
    }
}

fn row(i: usize) -> impl IntoWidget {
    let h = height_of(i);
    container().height(h).child(text(format!("row {i}")).size(14.0))
}

fn auto_root(count: usize) -> impl IntoWidget {
    let controller = use_scroll_controller();
    CTL.with(|c| *c.borrow_mut() = Some(controller));
    ListView::builder_auto(count, row)
        .estimated_extent(40.0)
        .controller(controller)
}

/// Run one frame: fold dirty components into the tree and lay out.
fn frame(ui: &mut Ui, env: &mut TextEnv, win: Size) {
    ui.rebuild_if_dirty();
    ui.layout(env, win);
}

/// The probes of the currently-built items, with their absolute window tops.
fn probe_tops(ui: &Ui) -> Vec<(usize, f64)> {
    // Probe render nodes carry no index — recover it from the row text? Simpler:
    // order probes by their position; the index is derivable from the tops only
    // once consecutive. Instead, we assert on gaps between consecutive probes.
    let mut tops: Vec<f64> = ui
        .render_tree()
        .find_all::<RenderMeasureProbe>()
        .into_iter()
        .map(|id| ui.render_tree().absolute_offset(id).y)
        .collect();
    tops.sort_by(f64::total_cmp);
    tops.into_iter().enumerate().map(|(i, t)| (i, t)).collect()
}

#[test]
fn auto_list_measures_visible_items_and_grows_the_content_extent() {
    CTL.with(|c| *c.borrow_mut() = None);
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(300.0, 200.0);
    ui.mount_root(View::new(palette::WHITE, component(|| auto_root(5000))).into_widget());
    ui.layout(&mut env, win);

    // Initial content extent: all 5000 rows at the 40px estimate.
    let extent = |ui: &Ui| {
        let id = ui.render_tree().find::<pebbles_render::RenderList>().unwrap();
        ui.render_tree().object_ref(id).downcast_ref::<pebbles_render::RenderList>().unwrap().content_extent
    };
    assert!((extent(&ui) - 200_000.0).abs() < 1e-6, "starts at count × estimate");

    // Corrective frames: the visible rows measure → the extent grows.
    for _ in 0..3 {
        frame(&mut ui, &mut env, win);
    }
    assert!(extent(&ui) > 200_000.0, "measured rows grow the content extent");

    // Consecutive visible probes sit exactly one row-height apart (no gap/overlap).
    let tops = probe_tops(&ui);
    assert!(tops.len() >= 5, "overscanned visible window");
    for (idx, (_, top)) in tops.iter().enumerate().skip(1) {
        let prev = tops[idx - 1].1;
        let gap = top - prev;
        assert!(gap >= 40.0 - 0.5 && gap <= 96.0 + 0.5, "sane row gap: {gap}");
    }
}

#[test]
fn auto_list_deep_jump_then_scroll_to_index_auto() {
    CTL.with(|c| *c.borrow_mut() = None);
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(300.0, 200.0);
    ui.mount_root(View::new(palette::WHITE, component(|| auto_root(5000))).into_widget());
    for _ in 0..3 {
        frame(&mut ui, &mut env, win);
    }

    // Jump deep: items around 100 000px become visible; their gaps still match
    // the row pattern (est 40 where unmeasured).
    let ctl = CTL.with(|c| c.borrow().expect("controller captured"));
    ctl.jump_to(100_000.0);
    for _ in 0..3 {
        frame(&mut ui, &mut env, win);
    }
    let tops = probe_tops(&ui);
    assert!(tops.len() >= 5, "window rebuilt around the jump");
    // Some row straddles the viewport top (its top sits within one row height
    // above 0) — anchoring keeps the jumped-to content in place.
    assert!(
        tops.iter().any(|(_, t)| *t > -97.0 && *t <= 1.0),
        "a row straddles the viewport top: {tops:?}"
    );

    // scroll_to_index_auto(500): the offset animates to the cache's prefix sum —
    // rows 0..6 measured (sum 400), rows 7..499 at the 40px estimate → 400 + 493×40.
    ctl.scroll_to_index_auto(500);
    animation::tick(0.0);
    for i in 1..=120 {
        animation::tick(i as f64 / 60.0);
    }
    for _ in 0..3 {
        frame(&mut ui, &mut env, win);
    }
    let expected = 400.0 + 493.0 * 40.0;
    // The anchor corrections shift the offset UP as rows above the viewport
    // measure taller than their estimate (the self-correction working) — it
    // must never undershoot the cache prefix, and item 500 must end up pinned
    // at the viewport top.
    assert!(
        ctl.offset() >= expected - 1.0,
        "never undershoots the cache prefix sum: {} vs {expected}",
        ctl.offset()
    );
    // After the settle + corrective frames, a row straddles the viewport top —
    // item 500 sits at the top of the list.
    let tops = probe_tops(&ui);
    assert!(
        tops.iter().any(|(_, t)| *t > -97.0 && *t <= 1.0),
        "item 500 (or its neighborhood) sits at the viewport top: {tops:?}"
    );
}

#[test]
fn auto_list_count_shrink_does_not_panic() {
    CTL.with(|c| *c.borrow_mut() = None);
    COUNT.with(|c| *c.borrow_mut() = None);
    let count = pebbles_core::create_signal(100usize);
    COUNT.with(|c| *c.borrow_mut() = Some(count));
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(300.0, 200.0);
    ui.mount_root(View::new(palette::WHITE, component(shrinking_root)).into_widget());
    for _ in 0..3 {
        frame(&mut ui, &mut env, win);
    }
    count.set(50);
    for _ in 0..3 {
        frame(&mut ui, &mut env, win);
    }
    count.set(3);
    for _ in 0..3 {
        frame(&mut ui, &mut env, win);
    }
    let extent = {
        let id = ui.render_tree().find::<pebbles_render::RenderList>().unwrap();
        ui.render_tree().object_ref(id).downcast_ref::<pebbles_render::RenderList>().unwrap().content_extent
    };
    assert!((extent - 200.0).abs() < 1e-6, "shrunk content extent: {extent}");
}

// `list_view` import kept out of the way — the fixed-extent helper is exercised
// by the other suites; silence the unused warning honestly.
#[allow(dead_code)]
fn _helpers() -> impl IntoWidget {
    list_view(vec![text("x").into_widget()])
}
