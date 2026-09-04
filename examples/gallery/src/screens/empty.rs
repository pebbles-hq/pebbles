use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn empty_screen() -> Element {
    screen("Empty")
        .description("The empty state — shadcn's Empty: an icon in a soft circle, a title, a description and an optional action. Every part is optional.")
        .body(children![
            doc("Full state")
                .description("Icon + title + description + action — the complete 'nothing here, here's what to do' block.")
                .body(
                    container()
                        .height(240.0)
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .child(
                            empty()
                                .icon(lucide::INBOX)
                                .title("No results found")
                                .description("Try a different search term, or clear the filters.")
                                .action(button("Clear filters").variant(ButtonVariant::Outline)),
                        ),
                ),
            doc("Minimal")
                .description("Just a title — the other parts hide themselves.")
                .body(
                    container()
                        .height(160.0)
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .child(empty().title("Nothing here")),
                ),
            doc("Icon gallery")
                .description("Any Lucide glyph works in the soft circle.")
                .body(
                    wrap(children![
                        mini_empty(lucide::SEARCH, "No matches"),
                        mini_empty(lucide::MAIL, "Inbox zero"),
                        mini_empty(lucide::STAR, "No favorites"),
                        mini_empty(lucide::CLOCK, "Nothing scheduled"),
                    ])
                    .spacing(10.0),
                ),
            doc("With an action")
                .description("Point the user at the next step with a button.")
                .body(
                    container()
                        .height(240.0)
                        .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                        .child(
                            empty()
                                .icon(lucide::FILE_PLUS)
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
