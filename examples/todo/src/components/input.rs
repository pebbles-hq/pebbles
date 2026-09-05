//! The add-todo field. Holds a tiny bit of **local** state — the draft text — that
//! only this component cares about. The field is **controlled** via `.bind(draft)`,
//! so submitting can clear it by setting the signal back to "".

use pebbles::prelude::*;

use crate::store;

pub fn input() -> impl IntoWidget {
    // Local state: the current draft. `.bind` two-way-binds it to the field.
    let draft = create_signal(String::new());

    let submit = move || {
        let text = draft.peek();
        store::add(text.trim());
        draft.set(String::new()); // clears the field (it's bound to `draft`)
    };

    row(children![
        Expanded::new(
            text_field().bind(draft).placeholder("What needs doing?").on_submit(move |_| submit()), // Enter also submits
        ),
        gap_w(10.0),
        button("Add").on_pressed(submit),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
}
