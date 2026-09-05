//! A stopwatch — start / stop / reset. `create_loop_while(running, period)` returns
//! a tick signal that advances only while `running` is true (and pauses when it's
//! false); the elapsed time is `ticks × period`. Reading it re-renders the display.

use pebbles::prelude::*;

/// The tick period (seconds) — the display resolution.
const PERIOD: f64 = 0.05;

fn stopwatch() -> impl IntoWidget {
    let running = create_signal(false);
    let ticks = create_loop_while(running.get(), PERIOD);

    let elapsed = ticks.get() * PERIOD;
    let secs = elapsed as u64;
    let clock = format!("{:02}:{:02}.{}", secs / 60, secs % 60, ((elapsed * 10.0) as u64) % 10);

    center(column(children![
        text("Stopwatch").size(20.0).color(palette::zinc::S600),
        gap_h(12.0),
        text(clock).size(64.0).weight(600.0).color(palette::zinc::S900).font_family("monospace"),
        gap_h(24.0),
        row(children![
            button(if running.get() { "Stop" } else { "Start" })
                .on_pressed(move || running.update(|r| *r = !*r)),
            gap_w(12.0),
            button("Reset").variant(ButtonVariant::Outline).on_pressed(move || {
                running.set(false);
                ticks.set(0.0);
            }),
        ])
        .main_axis_size(MainAxisSize::Min),
    ]))
}

#[pebbles::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(component(stopwatch))
        .title("Pebbles — Stopwatch")
        .size(420, 360)
        .background(palette::zinc::S50)
        .run()
}
