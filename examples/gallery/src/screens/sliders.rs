use pebbles::prelude::*;

use crate::ui::{doc, gap_h, screen};

pub fn sliders() -> Element {
    screen("Slider")
        .description("A draggable value slider (shadcn style) — a real min/max/step domain, one or two thumbs, horizontal or vertical, and fully keyboard-operable.")
        .body(
        children![basic(), range(), steps(), vertical(), disabled_slider()],
    )
}

fn basic() -> impl IntoWidget {
    let vol = create_signal(60.0);
    doc("Default")
        .description("A single thumb over the default 0–100 domain. Drag the thumb or click the track; once focused, the arrow keys step it and Home/End jump to the ends.")
        .body(
        column(children![
            slider(320.0).value(60.0).on_changed(move |v| vol.set(v[0])),
            gap_h(12.0),
            muted(format!("Value: {:.0}", vol.get())),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min),
    )
}

fn range() -> impl IntoWidget {
    let band = create_signal((20.0, 80.0));
    doc("Range")
        .description("Pass .range(lo, hi) for a two-thumb range selector. Clicking or dragging moves the nearest thumb, and neither can cross the other.")
        .body(
        column(children![
            slider(320.0).range(20.0, 80.0).on_changed(move |v| band.set((v[0], v[1]))),
            gap_h(12.0),
            muted(format!("{:.0} – {:.0}", band.get().0, band.get().1)),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min),
    )
}

fn steps() -> impl IntoWidget {
    doc("Steps & domain")
        .description("Snap to a .step() over any .main_axis_size(MainAxisSize::Min)/.max() domain. Here a 0–10 slider snapping to whole numbers, then a continuous 0–1 slider (step 0).")
        .body(
        column(children![
            slider(320.0).min(0.0).max(10.0).step(1.0).value(4.0),
            gap_h(20.0),
            slider(320.0).min(0.0).max(1.0).step(0.0).value(0.5),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min),
    )
}

fn vertical() -> impl IntoWidget {
    doc("Vertical")
        .description("Orient vertically with .orientation(Axis::Vertical) — the thumb travels from the bottom (min) to the top (max). Single and range both work.")
        .body(
        row(children![
            slider(150.0).orientation(Axis::Vertical).value(30.0),
            slider(150.0).orientation(Axis::Vertical).range(25.0, 70.0),
            slider(150.0).orientation(Axis::Vertical).value(85.0),
        ])
        .main_axis_size(MainAxisSize::Min)
        .spacing(32.0),
    )
}

fn disabled_slider() -> impl IntoWidget {
    doc("Disabled")
        .description("Non-interactive and dimmed via .disabled(true) — no drag, no keyboard, not-allowed cursor.")
        .body(
        slider(320.0).value(40.0).disabled(true),
    )
}
