//! M30 AC1 end to end: a real widget tree painted the way the shell runner paints it (the
//! UI scene re-recorded every frame and appended under the DPI scale), rendered through a
//! damage-tracking target. Repaint boundaries keep unchanged subtrees as the SAME fragments,
//! so a counter increment or a caret blink redraws a small rect — and the pixels still equal
//! a full redraw. Needs a GPU (`--ignored`; lavapipe in CI).

#![cfg(not(target_family = "wasm"))]

use kurbo::Affine;
use pebbles_core::{IntoWidget, Signal, Ui, animation, component, create_signal};
use pebbles_foundation::{Size, palette};
use pebbles_render::{Scene, TextEnv};
use pebbles_shell::capture::OffscreenRenderer;
use pebbles_widgets::{OverlayHost, View, card, column, text, text_field};

const W: u32 = 480;
const H: u32 = 360;

/// The runner's frame shape: `scene.reset(); ui.paint(..); frame.reset(); frame.append(scene, scale)`.
struct Harness {
    ui: Ui,
    env: TextEnv,
    scene: Scene,
    frame: Scene,
    damage: OffscreenRenderer,
    full: OffscreenRenderer,
}

impl Harness {
    fn new(root: impl IntoWidget) -> Option<Self> {
        let Ok(mut damage) = OffscreenRenderer::new() else { return None };
        let full = OffscreenRenderer::new().ok()?;
        damage.set_damage_tracking(true);
        pebbles_widgets::overlay::init();
        pebbles_core::focus::init();
        animation::reset();
        let mut ui = Ui::new();
        ui.make_current();
        ui.mount_root(View::new(palette::WHITE, OverlayHost::wrap(root.into_widget())).into_widget());
        pebbles_widgets::overlay::set_window_size(f64::from(W), f64::from(H));
        Some(Self { ui, env: TextEnv::new(), scene: Scene::new(), frame: Scene::new(), damage, full })
    }

    /// One frame; returns (damage_px, full_redraws) and checks the pixels against a full redraw.
    fn frame(&mut self) -> (u64, u32) {
        self.ui.make_current();
        self.ui.rebuild_if_dirty();
        self.ui.layout(&mut self.env, Size::new(f64::from(W), f64::from(H)));
        self.scene.reset();
        self.ui.paint(&mut self.env, &mut self.scene);
        self.frame.reset();
        self.frame.append(&self.scene, Some(Affine::scale(1.0)));
        let got = self.damage.render_rgba(&self.frame, (W, H), palette::WHITE, 1.0).expect("damage frame");
        let stats = self.damage.last_frame_stats();
        let want = self.full.render_rgba(&self.frame, (W, H), palette::WHITE, 1.0).expect("full frame");
        assert!(got.pixels == want.pixels, "a damage-tracked frame must equal the full redraw");
        (stats.damage_px, stats.full_redraws)
    }
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

fn cards() -> Vec<pebbles_core::AnyWidget> {
    (0..4).map(|i| card().title(format!("Card {i}")).description("Unchanged content").into_widget()).collect()
}

const AREA: u64 = W as u64 * H as u64;

#[test]
#[ignore = "gpu"]
fn counter_increment_damages_a_small_rect() {
    let mut rows = vec![component(|| text(format!("Count: {}", count().get()))).into_widget()];
    rows.extend(cards());
    let Some(mut h) = Harness::new(column(rows)) else {
        eprintln!("counter_increment_damages_a_small_rect: no adapter, skipped");
        return;
    };
    count().set(0);
    let (_, full) = h.frame();
    assert_eq!(full, 1, "the first frame is a full redraw");
    assert_eq!(h.frame(), (0, 0), "an unchanged frame redraws nothing");
    for n in 1..=3 {
        count().set(n);
        let (px, full) = h.frame();
        eprintln!("counter {n}: damage_px={px} ({:.2}% of the window)", 100.0 * px as f64 / AREA as f64);
        assert_eq!(full, 0, "an increment is a partial frame");
        assert!(px > 0 && px * 20 < AREA, "an increment damages under 5% of the window: {px} px");
    }
}

#[test]
#[ignore = "gpu"]
fn caret_blink_damages_a_small_rect() {
    let mut rows = vec![text_field().autofocus().width(240.0).into_widget()];
    rows.extend(cards());
    let Some(mut h) = Harness::new(column(rows)) else {
        eprintln!("caret_blink_damages_a_small_rect: no adapter, skipped");
        return;
    };
    // Autofocus lands after the first frame; let the focus ring settle, then step through
    // blink phases (1 s cycle).
    for t in [0.0, 1.0, 2.0, 3.0] {
        animation::tick(t);
        h.frame();
    }
    let mut blinks = 0;
    for i in 1..=8 {
        animation::tick(3.0 + 0.27 * f64::from(i)); // absolute animation time
        let (px, full) = h.frame();
        eprintln!("blink: damage_px={px} ({:.2}% of the window)", 100.0 * px as f64 / AREA as f64);
        assert_eq!(full, 0, "a blink is a partial frame");
        assert!(px * 100 < AREA, "a blink damages under 1% of the window: {px} px");
        blinks += usize::from(px > 0);
    }
    assert!(blinks >= 3, "the caret blinked ({blinks} damaged frames)");
}

/// GPU time per frame for a counter app (run with `PEBBLES_GPU_TIMING=1`, and again with
/// `PEBBLES_GPU_DAMAGE=0` for the full-redraw baseline). Prints; asserts nothing on timing.
#[test]
#[ignore = "gpu"]
fn measure_gpu_time_counter() {
    let mut rows = vec![component(|| text(format!("Count: {}", count().get()))).into_widget()];
    rows.extend(cards());
    let Some(mut h) = Harness::new(column(rows)) else { return };
    let mut gpu = Vec::new();
    for n in 0..240 {
        count().set(n);
        h.ui.make_current();
        h.ui.rebuild_if_dirty();
        h.ui.layout(&mut h.env, Size::new(f64::from(W), f64::from(H)));
        h.scene.reset();
        h.ui.paint(&mut h.env, &mut h.scene);
        h.frame.reset();
        h.frame.append(&h.scene, Some(Affine::scale(1.0)));
        h.damage.render_rgba(&h.frame, (W, H), palette::WHITE, 1.0).expect("frame");
        if let Some(us) = h.damage.last_frame_stats().gpu_us
            && n >= 40
        {
            gpu.push(us);
        }
    }
    gpu.sort_unstable();
    if gpu.is_empty() {
        eprintln!("measure_gpu_time_counter: no gpu_us (set PEBBLES_GPU_TIMING=1)");
        return;
    }
    let mean = gpu.iter().map(|&u| u64::from(u)).sum::<u64>() / gpu.len() as u64;
    eprintln!(
        "GPU_TIME damage={} n={} mean={mean}us p50={}us p95={}us",
        std::env::var("PEBBLES_GPU_DAMAGE").unwrap_or_else(|_| "1".into()),
        gpu.len(),
        gpu[gpu.len() / 2],
        gpu[gpu.len() * 95 / 100]
    );
}
