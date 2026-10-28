use pebbles::prelude::*;

use crate::ui::{doc, gap_h, screen};

pub fn sliders() -> Element {
    screen("Slider").body(children![basic(), range(), steps(), vertical(), disabled_slider()])
}

fn basic() -> impl IntoWidget {
    let vol = create_signal(60.0);
    doc("Default").body(
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
    doc("Range").body(
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
    doc("Steps & domain").body(
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
    doc("Vertical").body(
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
    doc("Disabled").body(slider(320.0).value(40.0).disabled(true))
}
