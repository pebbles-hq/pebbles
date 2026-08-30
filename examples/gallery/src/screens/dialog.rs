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
                SizedBox::spacer(0.0, 6.0),
                muted(desc.to_string()),
                SizedBox::spacer(0.0, 18.0),
                body_w,
                SizedBox::spacer(0.0, 22.0),
                row(children![spacer(), footer]),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_min(),
        )
        .into_widget()
}

pub fn dialogs_screen() -> impl IntoWidget {
    let status = create_signal(String::from("—"));
    screen(
        "Dialog",
        "A modal that opens in its OWN OS window (winit multi-window). Close it with the footer button, the Escape key, or the window's own close control — each fires on_close.",
        children![basic(status), form(status), confirm(status), sized(status), status_line(status)],
    )
}

fn status_line(status: Signal<String>) -> impl IntoWidget {
    doc("Last event", "Reflects the most recent dialog result.", muted(format!("→ {}", status.get())))
}

fn basic(status: Signal<String>) -> impl IntoWidget {
    doc(
        "Basic",
        "Opens a real window. It shares the reactive runtime, so its widgets are fully live.",
        button("Open dialog").on_pressed(action(move || {
            let idc = Rc::new(Cell::new(0u64));
            let idc2 = idc.clone();
            let content = panel(
                "Welcome to Pebbles",
                "This is a separate operating-system window, rendered by the same engine.",
                body("Anything you build works here — text, buttons, inputs, the lot."),
                button("Close")
                    .variant(ButtonVariant::Secondary)
                    .on_pressed(action(move || close_dialog(idc2.get()))),
            );
            let id = dialog(content)
                .title("Welcome")
                .size(440, 300)
                .on_close(move || status.set("basic dialog closed".into()))
                .open();
            idc.set(id);
            status.set("basic dialog opened".into());
        })),
    )
}

fn form(status: Signal<String>) -> impl IntoWidget {
    doc(
        "With a form",
        "Inputs work inside a dialog — type, focus with Tab, and submit.",
        button("Edit profile").variant(ButtonVariant::Outline).on_pressed(action(move || {
            let name = create_signal(String::from("Reyco"));
            let idc = Rc::new(Cell::new(0u64));
            let close_a = idc.clone();
            let close_b = idc.clone();
            let content = panel(
                "Edit profile",
                "Make changes to your profile here. Click save when you're done.",
                column(children![
                    label("Name"),
                    SizedBox::spacer(0.0, 6.0),
                    text_field().bind(name).width(300.0),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_min(),
                row(children![
                    button("Cancel")
                        .variant(ButtonVariant::Ghost)
                        .on_pressed(action(move || close_dialog(close_a.get()))),
                    gap_w(8.0),
                    button("Save changes").on_pressed(action(move || {
                        status.set(format!("saved: {}", name.get()));
                        close_dialog(close_b.get());
                    })),
                ])
                .main_axis_min(),
            );
            let id = dialog(content).title("Edit profile").size(420, 320).open();
            idc.set(id);
        })),
    )
}

fn confirm(status: Signal<String>) -> impl IntoWidget {
    doc(
        "Confirmation",
        "A destructive confirm — Cancel or Delete, either closes the window.",
        button("Delete account").variant(ButtonVariant::Destructive).on_pressed(action(move || {
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
                        .on_pressed(action(move || close_dialog(cancel.get()))),
                    gap_w(8.0),
                    button("Delete").variant(ButtonVariant::Destructive).on_pressed(action(move || {
                        status.set("account deleted".into());
                        close_dialog(del.get());
                    })),
                ])
                .main_axis_min(),
            );
            let id = dialog(content).title("Confirm deletion").size(440, 260).open();
            idc.set(id);
        })),
    )
}

fn sized(status: Signal<String>) -> impl IntoWidget {
    doc(
        "Custom size",
        "Size the window with .size(w, h) and set its title.",
        button("Open large").variant(ButtonVariant::Secondary).on_pressed(action(move || {
            let idc = Rc::new(Cell::new(0u64));
            let idc2 = idc.clone();
            let content = panel(
                "A larger canvas",
                "Dialogs are ordinary windows — make them as big as you need.",
                body("Great for editors, inspectors and detachable panels (hello, Gravel)."),
                button("Done")
                    .on_pressed(action(move || close_dialog(idc2.get()))),
            );
            let id = dialog(content)
                .title("Large dialog")
                .size(680, 460)
                .on_close(move || status.set("large dialog closed".into()))
                .open();
            idc.set(id);
        })),
    )
}
