//! A two-way temperature converter — type in either field, the other updates.
//! Two signals, two `on_changed` handlers; setting one field's value doesn't fire
//! the other's handler, so there's no feedback loop to guard against.

use pebbles::prelude::*;

fn converter() -> impl IntoWidget {
    let celsius = create_signal(String::new());
    let fahrenheit = create_signal(String::new());

    let field =
        |label: &str, unit: &str, value: Signal<String>, on: Box<dyn Fn(&str)>| {
            row(children![
                text(label.to_string()).size(15.0).color(palette::zinc::S600),
                gap_w(12.0),
                Expanded::new(
                    text_field().kind(InputKind::Number).value(value.get()).on_changed(move |s| on(s)),
                ),
                gap_w(8.0),
                text(unit.to_string()).size(15.0).color(palette::zinc::S400),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Center)
        };

    center(
        container().width(360.0).child(
            column(children![
                text("Temperature").size(22.0).semibold().color(palette::zinc::S900),
                gap_h(20.0),
                field(
                    "Celsius",
                    "°C",
                    celsius,
                    Box::new(move |s: &str| {
                        celsius.set(s.to_string());
                        match s.parse::<f64>() {
                            Ok(c) => fahrenheit.set(format!("{:.1}", c * 9.0 / 5.0 + 32.0)),
                            Err(_) if s.is_empty() => fahrenheit.set(String::new()),
                            Err(_) => {}
                        }
                    }),
                ),
                gap_h(12.0),
                field(
                    "Fahrenheit",
                    "°F",
                    fahrenheit,
                    Box::new(move |s: &str| {
                        fahrenheit.set(s.to_string());
                        match s.parse::<f64>() {
                            Ok(f) => celsius.set(format!("{:.1}", (f - 32.0) * 5.0 / 9.0)),
                            Err(_) if s.is_empty() => celsius.set(String::new()),
                            Err(_) => {}
                        }
                    }),
                ),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min),
        ),
    )
}

#[pebbles::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(component(converter))
        .title("Pebbles — Temperature")
        .size(440, 320)
        .background(palette::zinc::S50)
        .run()
}
