use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn button_groups() -> Element {
    screen("Button Group").body(children![segmented(), icons(), vertical(), split(), spaced()])
}

fn segmented() -> impl IntoWidget {
    doc("Segmented").body(button_group(vec![
        button("Left").variant(ButtonVariant::Secondary),
        button("Center").variant(ButtonVariant::Secondary),
        button("Right").variant(ButtonVariant::Secondary),
    ]))
}

fn icons() -> impl IntoWidget {
    doc("Icon toolbar").body(button_group(vec![
        button("").variant(ButtonVariant::Ghost).leading(IconKind::ChevronLeft),
        button("").variant(ButtonVariant::Ghost).leading(IconKind::ChevronRight),
        button("").variant(ButtonVariant::Ghost).leading(IconKind::Plus),
        button("").variant(ButtonVariant::Ghost).leading(IconKind::Search),
    ]))
}

fn vertical() -> impl IntoWidget {
    doc("Vertical").body(
        button_group(vec![
            button("Profile").variant(ButtonVariant::Ghost).leading(IconKind::User),
            button("Mail").variant(ButtonVariant::Ghost).leading(IconKind::Mail),
            button("Settings").variant(ButtonVariant::Ghost).leading(IconKind::Menu),
        ])
        .orientation(Axis::Vertical),
    )
}

fn split() -> impl IntoWidget {
    doc("Split button").body(button_group(vec![
        button("Save").variant(ButtonVariant::Secondary),
        button("").variant(ButtonVariant::Secondary).leading(IconKind::ChevronDown),
    ]))
}

fn spaced() -> impl IntoWidget {
    doc("Spaced toolbar").body(
        button_group(vec![
            button("Bold").variant(ButtonVariant::Outline),
            button("Italic").variant(ButtonVariant::Outline),
            button("Underline").variant(ButtonVariant::Outline),
        ])
        .spacing(8.0),
    )
}
