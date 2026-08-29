use pebbles::prelude::*;

use crate::ui::{hstack, screen, section, vstack};

pub fn toggles() -> impl IntoWidget {
    let terms = create_signal(true);
    let notify = create_signal(false);
    let airplane = create_signal(true);
    let bold = create_signal(true);
    let plan = create_signal(1usize);

    let plans: Vec<_> = ["Free", "Pro", "Enterprise"]
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            hstack(
                children![radio(plan.get() == i).on_selected(action(move || plan.set(i))), label(name)],
                10.0,
            )
        })
        .collect();

    screen(
        "Toggles",
        "Checkbox, switch, radio and toggle — all animated on change.",
        children![
            section(
                "CHECKBOX & SWITCH",
                vstack(
                    children![
                        hstack(children![checkbox(terms.get()).on_changed(action(move || terms.update(|v| *v = !*v))), label("Accept terms")], 10.0),
                        hstack(children![checkbox(notify.get()).on_changed(action(move || notify.update(|v| *v = !*v))), label("Email notifications")], 10.0),
                        hstack(children![switch(airplane.get()).on_changed(action(move || airplane.update(|v| *v = !*v))), label("Airplane mode")], 10.0),
                    ],
                    12.0,
                ),
            ),
            section("RADIO GROUP", vstack(plans, 10.0)),
            section(
                "TOGGLE",
                hstack(
                    children![
                        toggle(bold.get(), text("B").bold()).on_changed(action(move || bold.update(|v| *v = !*v))),
                        label("Bold"),
                    ],
                    10.0,
                ),
            ),
        ],
    )
}
