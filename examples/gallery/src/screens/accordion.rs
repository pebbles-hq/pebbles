use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn accordions() -> Element {
    let note = create_signal(String::from("—"));

    screen("Accordion")
        .description("A stack of collapsible sections — self-managed (seed it with .default_open, read toggles through .on_toggle), single-open by default, with a tweening chevron.")
        .body(children![
            doc("Single-open — the default")
                .description("Opening one section collapses the siblings; .on_toggle(usize, bool) reports every change.")
                .body(
                    column(children![
                        accordion()
                            .item("Is it accessible?", muted("Yes. It follows the box protocol."))
                            .item("Is it styled?", muted("Yes, from theme tokens."))
                            .item("Is it animated?", muted("Yes — the chevron rotates."))
                            .default_open(0)
                            .on_toggle(move |i, open| note.set(format!(
                                "section {} → {}",
                                i + 1,
                                if open { "open" } else { "closed" }
                            ))),
                        gap_h(8.0),
                        muted(format!("last toggle: {}", note.get())),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("Multiple mode")
                .description(".multiple(true) keeps several sections open at once.")
                .body(
                    accordion()
                        .multiple(true)
                        .item("Billing", muted("Invoices, receipts, plans."))
                        .item("Team", muted("Members, roles, invites."))
                        .item("API keys", muted("Create and revoke tokens."))
                        .default_open(0)
                        .default_open(2),
                ),
            doc("Styled sections")
                .description("A Style covers every header — background, border, radius — and its text props style the titles. Hover feedback still mixes on top.")
                .body(
                    column(children![
                        accordion()
                            .item("Framed", muted("Header bg + border + radius."))
                            .item("Sharp", muted("radius 0 reads corporate."))
                            .style(
                                style()
                                    .background(theme().colors.card)
                                    .border(Border::new(theme().colors.border, 1.0))
                                    .radius_all(theme().radius),
                            ),
                        gap_h(16.0),
                        accordion()
                            .item("Accent titles", muted("The title takes the style color."))
                            .item("Bold titles", muted("weight and size follow too."))
                            .default_open(0)
                            .style(
                                style()
                                    .color(palette::emerald::S600)
                                    .font_weight(700.0)
                                    .font_size(15.0),
                            ),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
        ])
}
