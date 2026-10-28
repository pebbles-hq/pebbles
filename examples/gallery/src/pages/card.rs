use pebbles::prelude::*;

use crate::ui::{doc, gap_w, screen};

pub fn cards() -> Element {
    screen("Card").body(children![simple(), with_header(), with_footer(), with_action(), composed()])
}

fn simple() -> impl IntoWidget {
    doc("Simple").body(
        container().width(360.0).child(
            card().child(body("A plain card. Drop any widget inside and it gets the elevated surface.")),
        ),
    )
}

fn with_header() -> impl IntoWidget {
    doc("Header").body(
        container().width(360.0).child(
            card()
                .title("Create project")
                .description("Deploy your new project in one click.")
                .child(body("Project settings and options would appear here.")),
        ),
    )
}

fn with_footer() -> impl IntoWidget {
    doc("Footer actions").body(
        container().width(360.0).child(
            card()
                .title("Delete account")
                .description("This action is permanent and cannot be undone.")
                .footer(
                    row(children![
                        button("Cancel").variant(ButtonVariant::Outline),
                        gap_w(10.0),
                        button("Delete").variant(ButtonVariant::Destructive),
                    ])
                    .main_axis_size(MainAxisSize::Min),
                ),
        ),
    )
}

fn with_action() -> impl IntoWidget {
    doc("Header action").body(
        container().width(360.0).child(
            card()
                .title("Team")
                .description("Manage who has access.")
                .action(icon_button(IconKind::Menu))
                .child(body("3 members · 2 pending invites")),
        ),
    )
}

fn composed() -> impl IntoWidget {
    doc("Composed").body(
        container().width(360.0).child(
            card()
                .action(badge("New").variant(BadgeVariant::Success))
                .child(
                    column(children![
                        row(children![
                            avatar("RS").color(palette::emerald::S600),
                            gap_w(12.0),
                            column(children![
                                text("Reyco Seguma").size(14.0).semibold(),
                                muted("Pushed 3 commits to main"),
                            ])
                            .cross_axis_alignment(CrossAxisAlignment::Start)
                            .main_axis_size(MainAxisSize::Min),
                        ])
                        .main_axis_size(MainAxisSize::Min),
                        gap_h(12.0),
                        body("“Slider now supports range + keyboard; progress got its own screen.”"),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                )
                .footer(
                    row(children![
                        button("View").size(ButtonSize::Sm).variant(ButtonVariant::Outline),
                        gap_w(8.0),
                        button("Dismiss").size(ButtonSize::Sm).variant(ButtonVariant::Ghost),
                    ])
                    .main_axis_size(MainAxisSize::Min),
                ),
        ),
    )
}
