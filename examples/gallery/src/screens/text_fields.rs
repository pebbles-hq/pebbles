use pebbles::prelude::*;

use crate::ui::{doc, gap_h, screen};

const W: f64 = 360.0;

pub fn text_fields() -> Element {
    let name = create_signal(String::new());
    let query = create_signal(String::new());
    let mail = create_signal(String::new());
    // Live validation: show an error once there's text without an "@".
    let mail_err =
        (!mail.get().is_empty() && !mail.get().contains('@')).then(|| "Enter a valid email address".to_string());

    screen(
        "Text Fields",
        "There is ONE text widget. Like Flutter's single TextField, the input type is a config — text_field().kind(InputKind::Email) — not a widget per type. The kind drives the character filter, leading icon, placeholder, formatting and any affordance (password eye, search clear).",
        children![
            doc(
                "Text — the base",
                "text_field() with no kind. Click to focus, then type — arrows/Home/End, Ctrl+A/C/X/V, undo, drag-select and double-click-word all work. .on_changed() reports every edit.",
                column(
                    children![
                        text_field().placeholder("Your name").width(W).on_changed(move |s| name.set(s.to_string())),
                        muted(format!("value: {}", name.get())),
                    ]).start().min().spacing(10.0),
            ),
            doc(
                "Password",
                "text_field().kind(InputKind::Password) — a lock icon and a built-in show/hide (eye) toggle, obscuring managed for you.",
                text_field().kind(InputKind::Password).width(W),
            ),
            doc(
                "Email",
                "kind(InputKind::Email) — an envelope icon and a no-spaces filter.",
                text_field().kind(InputKind::Email).width(W),
            ),
            doc(
                "Number & currency",
                "kind(InputKind::Number) accepts digits, a decimal point and a minus sign. kind(InputKind::Currency) also groups thousands and prefixes $ as you type.",
                column(
                    children![
                        text_field().kind(InputKind::Number).placeholder("Amount").width(W),
                        gap_h(12.0),
                        text_field().kind(InputKind::Currency).width(W),
                    ]).start().min().spacing(0.0),
            ),
            doc(
                "Search",
                "kind(InputKind::Search) — a leading magnifier and a clear (×) button that appears once there's text.",
                column(
                    children![
                        text_field().kind(InputKind::Search).width(W).on_changed(move |s| query.set(s.to_string())),
                        muted(format!("query: {}", query.get())),
                    ]).start().min().spacing(10.0),
            ),
            doc(
                "Label, helper & validation",
                "The kind composes with everything else — here Email plus the shadcn form-field shape: a label above, helper below, and an error state. Type letters without an @ to see the error.",
                column(
                    children![
                        text_field()
                            .kind(InputKind::Email)
                            .label("Email")
                            .helper("We'll never share your email.")
                            .width(W)
                            .on_changed(move |s| mail.set(s.to_string()))
                            .error_opt(mail_err),
                    ]).start().min().spacing(0.0),
            ),
            doc(
                "Disabled",
                "Dimmed, non-interactive and not focusable via .disabled(true).",
                text_field().label("Account ID").value("acct_9f3a1c").disabled(true).width(W),
            ),
            doc(
                "URL & phone",
                "kind(InputKind::Url) blocks spaces; kind(InputKind::Phone) allows digits and phone punctuation with a phone icon.",
                column(
                    children![
                        text_field().kind(InputKind::Url).width(W),
                        gap_h(12.0),
                        text_field().kind(InputKind::Phone).width(W),
                    ]).start().min().spacing(0.0),
            ),
            doc(
                "Character limit",
                "Cap the length with .max_length(); typing stops at the limit. Here, 12 characters.",
                text_field().placeholder("Max 12 chars").max_length(12).width(W),
            ),
            doc(
                "Multiline (textarea)",
                "text_area(lines) grows to the given number of rows; Enter inserts a newline and the caret navigates by line.",
                text_area(4).placeholder("Write a description…").width(460.0),
            ),
            doc(
                "Field — labeled wrapper",
                "field(control) puts a label above, and a muted description or (via error_opt(Some)) a red error below — around ANY control, not just text inputs.",
                column(children![
                    field(text_field().kind(InputKind::Email).width(W))
                        .label("Email")
                        .description("We'll never share it."),
                    gap_h(16.0),
                    field(text_field().width(W))
                        .label("Username")
                        .error_opt(Some("That username is taken")),
                ])
                .start()
                .min(),
            ),
        ],
    )
}
