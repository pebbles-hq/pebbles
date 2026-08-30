use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn button_groups() -> impl IntoWidget {
    screen(
        "Button Group",
        "Join buttons into a single segmented control (shadcn's Button Group): the outer corners round, inner ones flatten, and a hairline divides each button. Horizontal or vertical, or spaced into a plain toolbar.",
        children![segmented(), icons(), vertical(), split(), spaced()],
    )
}

fn segmented() -> impl IntoWidget {
    doc(
        "Segmented",
        "A joined row of buttons — the classic segmented control. Pass any Buttons; the group flattens their radii and clips the ends.",
        button_group(vec![
            button("Left").variant(ButtonVariant::Secondary),
            button("Center").variant(ButtonVariant::Secondary),
            button("Right").variant(ButtonVariant::Secondary),
        ]),
    )
}

fn icons() -> impl IntoWidget {
    doc(
        "Icon toolbar",
        "Icon-only buttons read as a tight toolbar when joined.",
        button_group(vec![
            button("").variant(ButtonVariant::Ghost).leading(IconKind::ChevronLeft),
            button("").variant(ButtonVariant::Ghost).leading(IconKind::ChevronRight),
            button("").variant(ButtonVariant::Ghost).leading(IconKind::Plus),
            button("").variant(ButtonVariant::Ghost).leading(IconKind::Search),
        ]),
    )
}

fn vertical() -> impl IntoWidget {
    doc(
        "Vertical",
        "Stack them with .orientation(Axis::Vertical) — dividers run horizontally.",
        button_group(vec![
            button("Profile").variant(ButtonVariant::Ghost).leading(IconKind::User),
            button("Mail").variant(ButtonVariant::Ghost).leading(IconKind::Mail),
            button("Settings").variant(ButtonVariant::Ghost).leading(IconKind::Menu),
        ])
        .orientation(Axis::Vertical),
    )
}

fn split() -> impl IntoWidget {
    doc(
        "Split button",
        "A primary action joined to a dropdown affordance — the common split-button pattern.",
        button_group(vec![
            button("Save").variant(ButtonVariant::Secondary),
            button("").variant(ButtonVariant::Secondary).leading(IconKind::ChevronDown),
        ]),
    )
}

fn spaced() -> impl IntoWidget {
    doc(
        "Spaced toolbar",
        "Pass .spacing(n) to detach the buttons — each keeps its own shape.",
        button_group(vec![
            button("Bold").variant(ButtonVariant::Outline),
            button("Italic").variant(ButtonVariant::Outline),
            button("Underline").variant(ButtonVariant::Outline),
        ])
        .spacing(8.0),
    )
}
