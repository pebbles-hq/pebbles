use std::cell::Cell;
use std::rc::Rc;

use pebbles::prelude::*;

use crate::ui::{doc, gap_w, screen};

/// A padded dialog panel: title, description, body, then a right-aligned footer.
fn panel(title: &str, desc: &str, body_w: impl IntoWidget, footer: impl IntoWidget) -> AnyWidget {
    Container::new()
        .padding(EdgeInsets::all(22.0))
        .child(
            column(children![
                text(title.to_string()).size(18.0).semibold(),
                gap_h(6.0),
                muted(desc.to_string()),
                gap_h(18.0),
                body_w,
                gap_h(22.0),
                row(children![spacer(), footer]),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min),
        )
        .into_widget()
}

pub fn dialogs_screen() -> Element {
    let status = create_signal(String::from("—"));
    screen(
        "Dialog",
        "A modal rendered in the overlay layer: a dimmed scrim over the app with a centered surface. Close it with the footer button, the Escape key, or an outside click — each fires on_close.",
        children![
            basic(status),
            form(status),
            confirm(status),
            sized(status),
            alert(status),
            status_line(status)
        ],
    )
}

fn alert(status: Signal<String>) -> impl IntoWidget {
    doc(
        "Alert dialog",
        "A confirm/cancel preset — non-dismissible by default (an explicit choice is required); the confirm button goes destructive on request.",
        button("Delete account").variant(ButtonVariant::Destructive).on_pressed(move || {
            alert_dialog("Are you absolutely sure?")
                .description("This permanently deletes your account and cannot be undone.")
                .confirm("Delete")
                .cancel("Cancel")
                .destructive(true)
                .on_confirm(move || status.set("alert: confirmed (deleted)".into()))
                .on_cancel(move || status.set("alert: cancelled".into()))
                .open();
        }),
    )
}

fn status_line(status: Signal<String>) -> impl IntoWidget {
    doc("Last event", "Reflects the most recent dialog result.", muted(format!("→ {}", status.get())))
}

fn basic(status: Signal<String>) -> impl IntoWidget {
    doc(
        "Basic",
        "A centered modal surface over a dimmed scrim; its widgets are fully live.",
        button("Open dialog").on_pressed(move || {
            let idc = Rc::new(Cell::new(0u64));
            let idc2 = idc.clone();
            let content = panel(
                "Welcome to Pebbles",
                "A modal surface, rendered in the app's overlay layer by the same engine.",
                body("Anything you build works here — text, buttons, inputs, the lot."),
                button("Close")
                    .variant(ButtonVariant::Secondary)
                    .on_pressed(move || close_dialog(idc2.get())),
            );
            let id = dialog(content)
                .title("Welcome")
                .width(440.0)
                .on_close(move || status.set("basic dialog closed".into()))
                .open();
            idc.set(id);
            status.set("basic dialog opened".into());
        }),
    )
}

fn form(status: Signal<String>) -> impl IntoWidget {
    doc(
        "With a form",
        "Inputs work inside a dialog — type, focus with Tab, and submit.",
        button("Edit profile").variant(ButtonVariant::Outline).on_pressed(move || {
            let name = create_signal(String::from("Reyco"));
            let idc = Rc::new(Cell::new(0u64));
            let close_a = idc.clone();
            let close_b = idc.clone();
            let content = panel(
                "Edit profile",
                "Make changes to your profile here. Click save when you're done.",
                column(children![
                    label("Name"),
                    gap_h(6.0),
                    text_field().bind(name).width(300.0),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min),
                row(children![
                    button("Cancel")
                        .variant(ButtonVariant::Ghost)
                        .on_pressed(move || close_dialog(close_a.get())),
                    gap_w(8.0),
                    button("Save changes").on_pressed(move || {
                        status.set(format!("saved: {}", name.get()));
                        close_dialog(close_b.get());
                    }),
                ])
                .main_axis_size(MainAxisSize::Min),
            );
            let id = dialog(content).title("Edit profile").width(420.0).open();
            idc.set(id);
        }),
    )
}

fn confirm(status: Signal<String>) -> impl IntoWidget {
    doc(
        "Confirmation",
        "A destructive confirm — Cancel or Delete, either closes the window.",
        button("Delete account").variant(ButtonVariant::Destructive).on_pressed(move || {
            let idc = Rc::new(Cell::new(0u64));
            let cancel = idc.clone();
            let del = idc.clone();
            let content = panel(
                "Are you absolutely sure?",
                "This permanently deletes your account and cannot be undone.",
                body("All of your data will be removed from our servers."),
                row(children![
                    button("Cancel")
                        .variant(ButtonVariant::Outline)
                        .on_pressed(move || close_dialog(cancel.get())),
                    gap_w(8.0),
                    button("Delete").variant(ButtonVariant::Destructive).on_pressed(move || {
                        status.set("account deleted".into());
                        close_dialog(del.get());
                    }),
                ])
                .main_axis_size(MainAxisSize::Min),
            );
            let id = dialog(content).title("Confirm deletion").width(440.0).open();
            idc.set(id);
        }),
    )
}

fn sized(status: Signal<String>) -> impl IntoWidget {
    doc(
        "Custom size",
        "Size the window with .size(w, h) and set its title.",
        button("Open large").variant(ButtonVariant::Secondary).on_pressed(move || {
            let idc = Rc::new(Cell::new(0u64));
            let idc2 = idc.clone();
            let content = panel(
                "A larger canvas",
                "Dialogs are ordinary windows — make them as big as you need.",
                body("Great for editors, inspectors and detachable panels (hello, Gravel)."),
                button("Done")
                    .on_pressed(move || close_dialog(idc2.get())),
            );
            let id = dialog(content)
                .title("Large dialog")
                .width(680.0)
                .on_close(move || status.set("large dialog closed".into()))
                .open();
            idc.set(id);
        }),
    )
}
