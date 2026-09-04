use pebbles::prelude::*;

use crate::ui::{doc, gap_h, screen};

pub fn progress_screen() -> Element {
    screen("Progress")
        .description("A linear progress bar (shadcn style) — a determinate fraction, a custom domain and thickness, colored indicators, or an animated indeterminate sweep.")
        .body(
        children![values(), live(), sizing(), colors(), indeterminate_bar()],
    )
}

fn values() -> impl IntoWidget {
    doc("Values").description("A fraction in 0–1 fills the track. Here 25%, 50% and 80%.").body(
        column(children![progress(0.25, 340.0), progress(0.5, 340.0), progress(0.8, 340.0),])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(16.0),
    )
}

fn live() -> impl IntoWidget {
    let p = create_signal(0.4);
    doc("Driven by a slider")
        .description("Progress is just a bound value — here a slider feeds it live.")
        .body(
            column(children![
                slider(340.0).value(40.0).on_changed(move |v| p.set(v[0] / 100.0)),
                gap_h(14.0),
                progress(p.get(), 340.0),
                gap_h(10.0),
                muted(format!("{:.0}%", p.get() * 100.0)),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        )
}

fn sizing() -> impl IntoWidget {
    doc("Domain & thickness")
        .description("Set .max() to use another domain (e.g. 0–100) and .thickness() to resize the bar.")
        .body(
            column(children![
                progress(70.0, 340.0).max(100.0),
                gap_h(18.0),
                progress(0.6, 340.0).thickness(4.0),
                gap_h(18.0),
                progress(0.6, 340.0).thickness(14.0),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        )
}

fn colors() -> impl IntoWidget {
    doc("Colors")
        .description("A custom .color() for the indicator — pair it with meaning (success, warning, danger).")
        .body(
            column(children![
                progress(0.7, 340.0).color(palette::emerald::S600),
                gap_h(16.0),
                progress(0.5, 340.0).color(palette::amber::S500),
                gap_h(16.0),
                progress(0.3, 340.0).color(palette::rose::S600),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        )
}

fn indeterminate_bar() -> impl IntoWidget {
    doc("Indeterminate")
        .description(
            "For unknown durations, .indeterminate() sweeps a segment across the track continuously.",
        )
        .body(
            column(children![
                progress(0.0, 340.0).indeterminate(),
                gap_h(16.0),
                progress(0.0, 340.0).indeterminate().color(palette::violet::S600),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        )
}
