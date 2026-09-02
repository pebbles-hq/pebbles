use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

pub fn animated_containers() -> Element {
    screen("Animated Container")
        .description(
            "A Container whose width, height, color, radius, padding, margin and opacity animate implicitly whenever their values change — rebuild with a new value and it tweens instead of jumping. Flutter's AnimatedContainer.",
        )
        .body(children![
            box_animation(),
            color_animation(),
            padding_radius(),
            curves(),
        ])
}

fn box_animation() -> impl IntoWidget {
    let big = create_signal(false);
    doc("Size")
        .description(".width(..)/.height(..) tween whenever the target changes — toggle the state and the box grows/shrinks over .duration(..).")
        .body(column(children![
            button(if big.get() { "Shrink" } else { "Grow" })
                .on_pressed(move || big.update(|b| *b = !*b)),
            gap_h(12.0),
            animated_container()
                .width(if big.get() { 320.0 } else { 120.0 })
                .height(if big.get() { 120.0 } else { 48.0 })
                .duration(0.4)
                .radius(BorderRadius::all(10.0)),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min))
}

fn color_animation() -> impl IntoWidget {
    let hot = create_signal(false);
    doc("Color & opacity")
        .description(".color(..) lerps between the previous and next colors; .opacity(..) fades in/out. Both restart smoothly when the target changes mid-flight.")
        .body(column(children![
            button(if hot.get() { "Cool down" } else { "Heat up" })
                .on_pressed(move || hot.update(|h| *h = !*h)),
            gap_h(12.0),
            row(children![
                animated_container()
                    .width(140.0)
                    .height(70.0)
                    .duration(0.5)
                    .color(if hot.get() { palette::red::S500 } else { palette::blue::S500 })
                    .child(center(text("color").color(palette::WHITE).size(12.5))),
                gap_w(16.0),
                animated_container()
                    .width(140.0)
                    .height(70.0)
                    .duration(0.5)
                    .opacity(if hot.get() { 1.0 } else { 0.25 })
                    .color(theme().colors.primary)
                    .child(center(text("opacity").color(palette::WHITE).size(12.5))),
            ])
            .main_axis_size(MainAxisSize::Min),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min))
}

fn padding_radius() -> impl IntoWidget {
    let padded = create_signal(false);
    doc("Padding, margin & radius")
        .description("Every box property tweens: padding and margin edge-by-edge, the four corner radii independently.")
        .body(column(children![
            button(if padded.get() { "Tighten" } else { "Loosen" })
                .on_pressed(move || padded.update(|p| *p = !*p)),
            gap_h(12.0),
            animated_container()
                .duration(0.45)
                .padding(if padded.get() { EdgeInsets::all(24.0) } else { EdgeInsets::all(4.0) })
                .margin(if padded.get() { EdgeInsets::all(0.0) } else { EdgeInsets::all(16.0) })
                .radius(if padded.get() { BorderRadius::all(24.0) } else { BorderRadius::all(4.0) })
                .color(theme().colors.secondary)
                .child(muted("animated padding · margin · radius")),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min))
}

fn curves() -> impl IntoWidget {
    let step = create_signal(0usize);
    const CURVES: [Curve; 5] =
        [Curve::Linear, Curve::EaseIn, Curve::EaseOut, Curve::EaseInOut, Curve::EaseOutCubic];
    doc("Curves")
        .description(".curve(..) picks the easing: Linear, EaseIn, EaseOut, EaseInOut, EaseOutCubic. Each press switches the curve AND retargets the bar, so you can compare the feel.")
        .body(column(children![
            row(children![
                button("Linear").on_pressed(move || step.set(0)),
                gap_w(8.0),
                button("EaseIn").on_pressed(move || step.set(1)),
                gap_w(8.0),
                button("EaseOut").on_pressed(move || step.set(2)),
                gap_w(8.0),
                button("EaseInOut").on_pressed(move || step.set(3)),
                gap_w(8.0),
                button("EaseOutCubic").on_pressed(move || step.set(4)),
            ])
            .main_axis_size(MainAxisSize::Min),
            gap_h(12.0),
            animated_container()
                .duration(0.6)
                .width(if step.get() % 2 == 0 { 300.0 } else { 160.0 })
                .height(36.0)
                .radius(BorderRadius::all(6.0))
                .color(theme().colors.primary)
                .curve(CURVES[step.get() % 5]),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min))
}
