use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn empty_screen() -> Element {
    screen("Empty").body(children![
        doc("Full state").body(
            container()
                .height(240.0)
                .decoration(
                    BoxDecoration::new()
                        .border(Border::new(theme().colors.border, 1.0))
                        .radius(BorderRadius::all(theme().radius))
                )
                .child(
                    empty()
                        .icon(tabler::INBOX)
                        .title("No results found")
                        .description("Try a different search term, or clear the filters.")
                        .action(button("Clear filters").variant(ButtonVariant::Outline)),
                ),
        ),
        doc("Minimal").body(
            container()
                .height(160.0)
                .decoration(
                    BoxDecoration::new()
                        .border(Border::new(theme().colors.border, 1.0))
                        .radius(BorderRadius::all(theme().radius))
                )
                .child(empty().title("Nothing here")),
        ),
        doc("Icon gallery").body(
            wrap(children![
                mini_empty(tabler::SEARCH, "No matches"),
                mini_empty(tabler::MAIL, "Inbox zero"),
                mini_empty(tabler::STAR, "No favorites"),
                mini_empty(tabler::CLOCK, "Nothing scheduled"),
            ])
            .spacing(10.0),
        ),
        doc("With an action").body(
            container()
                .height(240.0)
                .decoration(
                    BoxDecoration::new()
                        .border(Border::new(theme().colors.border, 1.0))
                        .radius(BorderRadius::all(theme().radius))
                )
                .child(
                    empty()
                        .icon(tabler::FILE_PLUS)
                        .title("No projects yet")
                        .description("Create your first project to get started.")
                        .action(button("New project")),
                ),
        ),
    ])
}

fn mini_empty(glyph: IconData, title: &str) -> impl IntoWidget {
    container()
        .width(180.0)
        .height(150.0)
        .decoration(
            BoxDecoration::new()
                .border(Border::new(theme().colors.border, 1.0))
                .radius(BorderRadius::all(theme().radius)),
        )
        .child(empty().icon(glyph).title(title.to_string()).description("A short hint lives here."))
}
