use pebbles::prelude::*;

use crate::ui::{doc, gap_h, screen};

pub fn typography() -> Element {
    screen("Typography")
        .description("Themed text presets with real font weights, and the bundled font families applied with .font_family(..).")
        .body(children![
            column(children![
                heading("Heading — 30 bold"),
                title("Title — 18 semibold"),
                body("Body — the quick brown fox jumps over the lazy dog."),
                label("Label — 13 medium"),
                muted("Muted — secondary text"),
                gap_h(10.0),
                row(children![
                    text("Regular 400").weight(400.0),
                    text("Medium 500").weight(500.0),
                    text("Semibold 600").weight(600.0),
                    text("Bold 700").weight(700.0),
                ])
                .main_axis_size(MainAxisSize::Min)
                .spacing(16.0),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(8.0),
            gap_h(20.0),
            doc("Font families")
                .description(".font_family(\"name\") picks any family — bundled or installed on this machine. The four bundled faces ship in the binary; browse every available family (with search) on the Fonts screen.")
                .body(column({
                    let mut items: Vec<AnyWidget> = Vec::new();
                    for name in builtins() {
                        items.push(
                            row(children![
                                Container::new()
                                    .width(130.0)
                                    .child(text(name.to_string()).size(13.0).semibold())
                                    .into_widget(),
                                text("The quick brown fox jumps over the lazy dog".to_string())
                                    .font_family(name.to_string())
                                    .size(15.0)
                                    .into_widget(),
                            ])
                            .main_axis_size(MainAxisSize::Min)
                            .into_widget(),
                        );
                    }
                    items
                })
                .main_axis_size(MainAxisSize::Min)
                .spacing(6.0)),
        ])
}
