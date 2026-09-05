//! A stopwatch — start / stop / reset, with real elapsed time.
//!
//! Two ideas:
//! - **`animation::now()`** is the framework's monotonic clock (seconds), fed by the
//!   shell — cross-platform (no `std::time::Instant`, which panics on web).
//! - **`create_loop_while(running, ..)`** re-renders this component every frame while
//!   running, so the readout ticks up; when stopped it stops re-rendering (and stops
//!   drawing frames — no idle cost). We ignore the loop's *value* and read the clock.

use pebbles::prelude::*;

/// Seconds since the framework started (the shell-driven monotonic clock).
fn now() -> f64 {
    pebbles::core::animation::now()
}

fn stopwatch() -> impl IntoWidget {
    let running = create_signal(false);
    // Elapsed = time banked from previous runs + (running ? time since last start : 0).
    let banked = create_signal(0.0_f64);
    let started_at = create_signal(0.0_f64);

    // While running, re-render every frame so the clock updates live.
    let _frame = create_loop_while(running.get(), 0.016);

    let elapsed = banked.get() + if running.get() { now() - started_at.get() } else { 0.0 };

    let toggle = move || {
        if running.peek() {
            banked.update(|b| *b += now() - started_at.peek()); // stop: bank the run
            running.set(false);
        } else {
            started_at.set(now()); // start: mark the new start point
            running.set(true);
        }
    };
    let reset = move || {
        running.set(false);
        banked.set(0.0);
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
                column(children![clock(elapsed), gap_h(28.0), controls(running.get(), toggle, reset)])
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .main_axis_size(MainAxisSize::Min),
            ),
    )
}

/// The MM:SS.t readout.
fn clock(elapsed: f64) -> impl IntoWidget {
    let secs = elapsed as u64;
    let tenths = ((elapsed * 10.0) as u64) % 10;
    text(format!("{:02}:{:02}.{}", secs / 60, secs % 60, tenths))
        .size(66.0)
        .weight(700.0)
        .font_family("monospace")
        .color(theme().colors.foreground)
}

/// Start/Stop + Reset. Pure UI — it just calls the two closures.
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
