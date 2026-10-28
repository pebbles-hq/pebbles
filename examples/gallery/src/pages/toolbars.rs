//! C8f — Toolbar & StatusBar: the thin chrome bars.

use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn toolbars() -> Element {
    screen("Toolbar & Status Bar").body(children![
        doc("Toolbar").body(toolbar(vec![
            button("New").into_widget(),
            gap_w(6.0).into_widget(),
            button("Open").into_widget(),
            gap_w(6.0).into_widget(),
            button("Save").into_widget(),
        ])),
        doc("Status bar").body(status_bar("Ready · 3 items · UTF-8 · Ln 1, Col 1")),
    ])
}
