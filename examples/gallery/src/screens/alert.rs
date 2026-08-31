use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn alerts() -> Element {
    screen("Alert")
        .description("A callout — shadcn's Alert: a variant-tinted surface with an icon, a title and an optional description.")
        .body(children![
            doc("Variants")
                .description("Info, Success, Warning and Destructive — each with its own icon and tint.")
                .body(
                    column(children![
                        alert("Heads up!").description("You can add components using the CLI."),
                        alert("Success").description("Your changes have been saved.").variant(AlertVariant::Success),
                        alert("Warning").description("This action cannot be undone.").variant(AlertVariant::Warning),
                        alert("Destructive").description("Your account will be permanently deleted.").variant(AlertVariant::Destructive),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min)
                    .spacing(10.0),
                ),
            doc("Title only")
                .description("The description is optional — a bare title still reads as a callout.")
                .body(alert("Connection restored")),
            doc("Long content")
                .description("Descriptions wrap — the callout grows with the text.")
                .body(
                    alert("Storage quota reached")
                        .description("Your workspace is at 98% of its 10 GB quota. New uploads will fail until you free space or upgrade your plan. Files are never deleted automatically.")
                        .variant(AlertVariant::Warning),
                ),
            doc("Styled")
                .description("A Style merges over the variant tint — custom accents stay legible.")
                .body(
                    column(children![
                        alert("Pebbles beta").description("Preview builds update daily.").style(style().background(theme().colors.card)),
                        alert("Custom accent").description("Radius + shadow from a Style.").variant(AlertVariant::Success).style(style().radius_all(4.0)),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min)
                    .spacing(10.0),
                ),
            doc("With an action")
                .description("Alerts have no built-in buttons — compose one underneath for the 'read + act' pattern.")
                .body(
                    column(children![
                        alert("Update available")
                            .description("Pebbles 0.2 is ready — restart to apply it.")
                            .variant(AlertVariant::Info),
                        gap_h(10.0),
                        row(children![
                            button("Restart now"),
                            button("Later").variant(ButtonVariant::Ghost),
                        ])
                        .main_axis_size(MainAxisSize::Min)
                        .spacing(10.0),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
        ])
}
