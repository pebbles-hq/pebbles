//! The counter — SolidJS-style, and now Flutter-clean: no `.into_widget()`, no
//! structs, no traits. A component is a function; state is a signal; handlers are
//! plain closures.

use pebbles::prelude::*;

fn counter() -> impl IntoWidget {
    let count = create_signal(0);

    center(column(children![
        text("Pebbles counter").size(20.0).color(palette::zinc::S600),
        gap_h(16.0),
        text(format!("{}", count.get())).size(72.0).color(palette::zinc::S900),
        gap_h(24.0),
        row(children![
            button("−").variant(ButtonVariant::Outline).on_pressed(move || count.update(|c| *c -= 1)),
            gap_w(16.0),
            button("+").on_pressed(move || count.update(|c| *c += 1)),
        ])
        .main_axis_size(MainAxisSize::Min),
    ]))
}

// `#[pebbles::main]` makes this the entry on every target: a plain `fn main` on
// desktop/web, plus the generated `android_main` on Android. No-op off Android.
#[pebbles::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(component(counter))
        .title("Pebbles — Counter")
        .size(480, 420)
        .background(palette::zinc::S50)
        .run()
}
