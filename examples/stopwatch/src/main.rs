//! A stopwatch — start / stop / reset, counting **real** elapsed time.
//!
//! It's also a tour of two reactivity primitives:
//! - **`create_effect`** (SolidJS `createEffect`) — a side effect that re-runs when a
//!   signal it reads changes. Here it runs once per frame *while running* and
//!   accumulates the real time delta into `elapsed`. Because a Pebbles effect is
//!   created ONCE and re-runs only on its tracked signals (not on every render), it
//!   can safely write a signal the view reads without looping.
//! - **`create_memo`** (SolidJS `createMemo`) — a cached derived value. `clock`
//!   derives the display string from `elapsed`.
//!
//! Time comes from `animation::now()` — the framework's monotonic clock, fed by the
//! shell (cross-platform; no `std::time::Instant`, which panics on web).

use pebbles::prelude::*;

/// Seconds since startup, from the shell-driven monotonic clock.
fn now() -> f64 {
    pebbles::core::animation::now()
}

fn stopwatch() -> impl IntoWidget {
    let running = create_signal(false);
    let elapsed = create_signal(0.0_f64); // accumulated seconds
    let last = create_signal(0.0_f64); // now() at the last counted frame; 0 = "start fresh"

    // A frame ticker: `create_loop_while` bumps a signal every ~16ms while `running`.
    let ticker = create_loop_while(running.get(), 0.016);

    // The effect runs on every ticker change (≈ every frame while running) and adds
    // the real time delta. It reads `running`/`last`/`elapsed` with `peek` (no
    // subscribe), so writing them never re-triggers it — it only re-runs on `ticker`.
    create_effect(move || {
        ticker.get(); // the one tracked read → wakes this effect each frame
        if !running.peek() {
            return;
        }
        let (t, prev) = (now(), last.peek());
        if prev <= 0.0 {
            last.set(t); // first frame of a run: record the (fresh) start, no delta yet
        } else {
            elapsed.update(|e| *e += (t - prev).max(0.0));
            last.set(t);
        }
    });

    // A memo: the formatted clock string, derived from `elapsed` and cached.
    let clock = create_memo(move || format_clock(elapsed.get()));

    let toggle = move || {
        running.update(|r| *r = !*r);
        last.set(0.0); // next run re-captures its start on the first frame
    };
    let reset = move || {
        running.set(false);
        elapsed.set(0.0);
        last.set(0.0);
    };

    let c = theme().colors;
    center(
        container()
            .decoration(
                BoxDecoration::new()
                    .color(c.card)
                    .border(Border::new(c.border, 1.0))
                    .radius(BorderRadius::all(20.0)),
            )
            .padding(EdgeInsets::symmetric(48.0, 36.0))
            .child(
                column(children![
                    text(clock.get()).size(66.0).weight(700.0).font_family("monospace").color(c.foreground),
                    gap_h(28.0),
                    controls(running.get(), toggle, reset),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Center)
                .main_axis_size(MainAxisSize::Min),
            ),
    )
}

/// Format seconds as `MM:SS.t`.
fn format_clock(elapsed: f64) -> String {
    let secs = elapsed as u64;
    let tenths = ((elapsed * 10.0) as u64) % 10;
    format!("{:02}:{:02}.{}", secs / 60, secs % 60, tenths)
}

/// Start/Stop + Reset — pure UI over the two closures.
fn controls(running: bool, toggle: impl Fn() + 'static, reset: impl Fn() + 'static) -> impl IntoWidget {
    row(children![
        button(if running { "Stop" } else { "Start" }).size(ButtonSize::Lg).on_pressed(toggle),
        gap_w(12.0),
        button("Reset").variant(ButtonVariant::Outline).size(ButtonSize::Lg).on_pressed(reset),
    ])
    .main_axis_size(MainAxisSize::Min)
}

#[pebbles::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(component(stopwatch))
        .title("Pebbles — Stopwatch")
        .size(440, 380)
        .background(theme().colors.background)
        .run()
}
