//! C8f — Toolbar & StatusBar: the thin chrome bars.

use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn toolbars() -> Element {
    screen("Toolbar & Status Bar")
        .description(
            "Thin chrome bars: a top Toolbar of actions (with a bottom border) and a bottom \
             StatusBar for ambient status text.",
        )
        .body(children![
            doc("Toolbar")
                .description("A horizontal action bar — pass any widgets (buttons, separators, …).")
                .body(toolbar(vec![
                    button("New").into_widget(),
                    gap_w(6.0).into_widget(),
                    button("Open").into_widget(),
                    gap_w(6.0).into_widget(),
                    button("Save").into_widget(),
                ])),
            doc("Status bar")
                .description("A muted footer strip for status text.")
                .body(status_bar("Ready · 3 items · UTF-8 · Ln 1, Col 1")),
        ])
}
