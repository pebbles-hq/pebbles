use pebbles::prelude::*;

use crate::ui::{screen, section, vstack};

pub fn selects() -> impl IntoWidget {
    let picked = create_signal(String::from("Pro"));

    screen(
        "Select",
        "A dropdown that opens in the overlay layer — flips up near the bottom edge, closes on scroll.",
        children![section(
            "SELECT",
            vstack(
                children![
                    select(["Free", "Pro", "Enterprise", "Team", "Startup", "Growth", "Custom"])
                        .width(260.0)
                        .value(1)
                        .placeholder("Choose a plan")
                        .on_changed(move |_i, label| picked.set(label.to_string())),
                    muted(format!("selected: {}", picked.get())),
                ],
                10.0,
            ),
        )],
    )
}
