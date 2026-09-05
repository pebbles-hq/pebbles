//! Temperature converter — **two-way binding**. Type in either field and the other
//! updates.
//!
//! The trick: each field is **controlled** via `.bind(signal)`. Typing fires
//! `on_changed`, where we convert and write the *other* signal — and because that
//! write is programmatic (not a keystroke) it does NOT fire the other field's
//! `on_changed`, so there's no feedback loop to guard against.

use pebbles::prelude::*;

fn converter() -> impl IntoWidget {
    // The source of truth for each field (as the user-entered text).
    let celsius = create_signal(String::new());
    let fahrenheit = create_signal(String::new());

    let c = theme().colors;
    center(
        container()
            .width(400.0)
            .decoration(
                BoxDecoration::new()
                    .color(c.card)
                    .border(Border::new(c.border, 1.0))
                    .radius(BorderRadius::all(18.0)),
            )
            .padding(EdgeInsets::all(24.0))
            .child(
                column(children![
                    text("Temperature").size(22.0).weight(700.0).color(c.foreground),
                    gap_h(20.0),
                    // Typing Celsius → set Fahrenheit.
                    field("Celsius", "°C", celsius, move |s| {
                        write_converted(s, fahrenheit, |v| v * 9.0 / 5.0 + 32.0)
                    }),
                    gap_h(12.0),
                    // Typing Fahrenheit → set Celsius.
                    field("Fahrenheit", "°F", fahrenheit, move |s| {
                        write_converted(s, celsius, |v| (v - 32.0) * 5.0 / 9.0)
                    }),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_size(MainAxisSize::Min),
            ),
    )
}

/// A labelled, number-only, controlled input row. `on_changed` fires on each edit.
fn field(
    label: &str,
    unit: &str,
    value: Signal<String>,
    on_changed: impl Fn(&str) + 'static,
) -> impl IntoWidget {
    let c = theme().colors;
    row(children![
        container().width(96.0).child(text(label.to_string()).size(15.0).color(c.muted_foreground)),
        Expanded::new(text_field().kind(InputKind::Number).bind(value).on_changed(on_changed)),
        gap_w(8.0),
        text(unit.to_string()).size(15.0).color(c.muted_foreground),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
}

/// Parse `s` and write `convert(n)` into `other` (or clear it when `s` is blank).
fn write_converted(s: &str, other: Signal<String>, convert: impl Fn(f64) -> f64) {
    let s = s.trim();
    if s.is_empty() {
        other.set(String::new());
    } else if let Ok(n) = s.parse::<f64>() {
        other.set(format!("{:.1}", convert(n)));
    }
    // A partial number (e.g. "-" or "1.") just leaves the other field as-is.
}

#[pebbles::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(component(converter))
        .title("Pebbles — Temperature")
        .size(460, 300)
        .background(theme().colors.background)
        .run()
}
