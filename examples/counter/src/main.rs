//! The counter — SolidJS-style, and now Flutter-clean: no `.into_widget()`, no
//! structs, no traits. A component is a function; state is a signal; handlers are
//! plain closures.

use pebbles::prelude::*;

fn counter() -> impl IntoWidget {
    let count = create_signal(0);

    center(column((
        text("Pebbles counter").size(20.0).color(palette::GREY_600),
        SizedBox::spacer(0.0, 16.0),
        text(format!("{}", count.get())).size(72.0).color(palette::GREY_900),
        SizedBox::spacer(0.0, 24.0),
        row((
            button("−")
                .variant(ButtonVariant::Outline)
                .on_pressed(move || count.update(|c| *c -= 1)),
            SizedBox::spacer(16.0, 0.0),
            button("+").on_pressed(move || count.update(|c| *c += 1)),
        ))
        .min(),
    )))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(component(counter))
        .title("Pebbles — Counter")
        .size(480, 420)
        .background(palette::GREY_50)
        .run()
}
