use pebbles::prelude::*;

use crate::ui::{screen, section, vstack};

pub fn sliders() -> impl IntoWidget {
    let vol = create_signal(0.6_f64);

    screen(
        "Slider",
        "A draggable value slider (shadcn style) plus determinate progress bars.",
        children![section(
            "SLIDER — drag the thumb or click the track",
            vstack(
                children![
                    slider(320.0).value(0.6).on_changed(move |t| vol.set(t)),
                    muted(format!("volume: {:.0}%", vol.get() * 100.0)),
                    progress(0.4, 320.0),
                    progress(0.85, 320.0),
                ],
                14.0,
            ),
        )],
    )
}
