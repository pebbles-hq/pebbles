use pebbles::prelude::*;

use crate::ui::{gap_h, screen};

pub fn typography() -> Element {
    screen(
        "Typography",
        "Themed text presets with real font weights.",
        children![column(
            children![
                heading("Heading — 30 bold"),
                title("Title — 18 semibold"),
                body("Body — the quick brown fox jumps over the lazy dog."),
                label("Label — 13 medium"),
                muted("Muted — secondary text"),
                gap_h(10.0),
                row(
                    children![
                        text("Regular 400").weight(400.0),
                        text("Medium 500").weight(500.0),
                        text("Semibold 600").weight(600.0),
                        text("Bold 700").weight(700.0),
                    ]).main_axis_size(MainAxisSize::Min).spacing(16.0),
            ]).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min).spacing(8.0)],
    )
}
