use pebbles::prelude::*;

use crate::ui::{gap_h, hstack, screen, vstack};

pub fn typography() -> impl IntoWidget {
    screen(
        "Typography",
        "Themed text presets with real font weights.",
        children![vstack(
            children![
                heading("Heading — 30 bold"),
                title("Title — 18 semibold"),
                body("Body — the quick brown fox jumps over the lazy dog."),
                label("Label — 13 medium"),
                muted("Muted — secondary text"),
                gap_h(10.0),
                hstack(
                    children![
                        text("Regular 400").weight(400.0),
                        text("Medium 500").weight(500.0),
                        text("Semibold 600").weight(600.0),
                        text("Bold 700").weight(700.0),
                    ],
                    16.0,
                ),
            ],
            8.0,
        )],
    )
}
