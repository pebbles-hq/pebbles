//! M20 (10 §9.2, B15): scrolling a 300-row list for 1000 frames leaves the GPU census flat.
//! Leaks are judged by `GpuCensus`, never RSS. Needs a GPU (`--ignored`; lavapipe in CI).
//!
//! The scroll cycles through the same 750 px (3 px per frame, 250 frames per cycle) and the
//! baseline is taken after the first full cycle: new rows bring new glyphs at new subpixel
//! offsets, which is content, not a leak — so the census is compared once every row in the
//! range has been seen.

#![cfg(all(not(target_family = "wasm"), debug_assertions))]

use pebbles_core::{IntoWidget, Ui};
use pebbles_foundation::{Size, palette};
use pebbles_render::{Scene, TextEnv};
use pebbles_shell::capture::OffscreenRenderer;
use pebbles_widgets::{ListView, OverlayHost, View, text, use_scroll_controller};

#[test]
#[ignore = "gpu"]
fn scroll_1000_frames_census_flat() {
    let Ok(mut gpu) = OffscreenRenderer::new() else {
        eprintln!("scroll_1000_frames_census_flat: no adapter, skipped");
        return;
    };
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let (w, h) = (320u32, 240u32);
    let mut env = TextEnv::new();
    let mut ui = Ui::new();
    ui.make_current();
    let controller = use_scroll_controller();
    let list = ListView::builder(300, 24.0, |i| text(format!("row {i}"))).controller(controller);
    ui.mount_root(View::new(palette::WHITE, OverlayHost::wrap(list.into_widget())).into_widget());
    pebbles_widgets::overlay::set_window_size(f64::from(w), f64::from(h));

    const CYCLE: u32 = 250;
    let mut baseline = None;
    for frame in 1..=1000u32 {
        ui.make_current();
        controller.jump_to(f64::from(frame % CYCLE) * 3.0);
        ui.rebuild_if_dirty();
        ui.layout(&mut env, Size::new(f64::from(w), f64::from(h)));
        let mut scene = Scene::new();
        ui.paint(&mut env, &mut scene);
        gpu.render_rgba(&scene, (w, h), palette::WHITE, 1.0).expect("frame renders");
        assert_eq!(gpu.last_frame_stats().glyph_overflow, 0, "frame {frame}: glyph atlas overflowed");
        if frame == CYCLE {
            baseline = Some(gpu.census());
        }
    }
    let (baseline, now) = (baseline.expect("first cycle"), gpu.census());
    let drift: Vec<String> = pebbles_gpu::GpuCensus::COLUMNS
        .iter()
        .zip(baseline.values().into_iter().zip(now.values()))
        .filter(|(_, (a, b))| a != b)
        .map(|(name, (a, b))| format!("{name}: {a} → {b} ({:+})", i128::from(b) - i128::from(a)))
        .collect();
    assert!(drift.is_empty(), "census at frame 1000 differs from frame {CYCLE}:\n{}", drift.join("\n"));
}
