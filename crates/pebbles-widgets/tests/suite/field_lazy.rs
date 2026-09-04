//! P5.2 — the windowed COLD build, end to end through the widget layer: a huge
//! `text_area` mounts by shaping O(window) lines (estimates elsewhere), scrolls
//! into never-shaped territory by materializing at paint, and settles its
//! estimate-then-measure corrective passes within a few frames.

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{View, container, scroll_view, text_area};

const LINES: usize = 6_000;

fn huge_source() -> String {
    let mut s = String::with_capacity(LINES * 24);
    for i in 0..LINES {
        s.push_str(&format!("source line {i} of many\n"));
    }
    s
}

fn root() -> impl IntoWidget {
    container().height(600.0).child(scroll_view(text_area(LINES as u32).value(huge_source()).width(500.0)))
}

/// One settled frame: rebuild + layout + paint, looping the corrective
/// relayout (estimate-then-measure) until paint stops requesting one.
fn settle(ui: &mut Ui, env: &mut TextEnv, win: Size) -> usize {
    let mut passes = 0;
    loop {
        ui.rebuild_if_dirty();
        ui.layout(env, win);
        let mut scene = pebbles_render::Scene::new();
        if !ui.paint(env, &mut scene) {
            return passes;
        }
        passes += 1;
        assert!(passes < 6, "corrective passes settle within a few frames");
    }
}

#[test]
fn huge_text_area_cold_mounts_windowed_and_scrolls_lazily() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    pebbles_widgets::theme::init();
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(640.0, 640.0);

    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    let t = std::time::Instant::now();
    settle(&mut ui, &mut env, win);
    let cold = t.elapsed();
    let cold_shapes = env.shape_cache_len();
    eprintln!("[perf field-lazy] cold mount: {cold:?}, shaped {cold_shapes} of {LINES} lines");
    // The cold mount shaped the caret window + the visible window — never the
    // whole document (pre-P5.2: all 6000 lines shaped up front).
    assert!(cold_shapes < 400, "cold mount shapes O(window), not O(document) ({cold_shapes} of {LINES})");

    // Scroll deep into never-shaped territory: the visible window materializes
    // at paint, geometry settles, nothing panics, and the tail STAYS estimates.
    // Headless has no frame clock, so drive the scroll spring by hand until the
    // offset lands (the shell's render loop does this via tick_scrolls).
    ui.dispatch_scroll(Offset::new(320.0, 300.0), 40_000.0);
    for _ in 0..600 {
        let scrolling = ui.tick_scrolls(1.0 / 60.0);
        settle(&mut ui, &mut env, win);
        if !scrolling {
            break;
        }
    }
    let after_scroll = env.shape_cache_len();
    eprintln!("[perf field-lazy] after deep scroll: shaped {after_scroll}");
    assert!(after_scroll > cold_shapes, "the scrolled-to window materialized");
    assert!(
        after_scroll < 1_000,
        "a deep scroll shapes the pass-through windows, not the document ({after_scroll})"
    );
}
