use pebbles::prelude::*;

use crate::ui::{doc, gap_h, screen, vstack};

const W: f64 = 360.0;

pub fn text_fields() -> impl IntoWidget {
    let name = create_signal(String::new());
    let query = create_signal(String::new());
    let mail = create_signal(String::new());
    // Live validation: show an error once there's text without an "@".
    let mail_err =
        (!mail.get().is_empty() && !mail.get().contains('@')).then(|| "Enter a valid email address".to_string());

    screen(
        "Text Fields",
        "Every single-line input type — text, password, email, number, search, date, url, phone — plus multiline. All built on one TextField with full editing, filtering, and icons.",
        children![
            doc(
                "Text",
                "The plain input. Click to focus, then type — arrows/Home/End, Ctrl+A/C/X/V, undo, drag-select and double-click-word all work. .on_changed() reports every edit.",
                vstack(
                    children![
                        text_field().placeholder("Your name").width(W).on_changed(move |s| name.set(s.to_string())),
                        muted(format!("value: {}", name.get())),
                    ],
                    10.0,
                ),
            ),
            doc(
                "Password",
                "Obscured entry with a built-in show/hide (eye) toggle and a lock icon. password_field() manages the visibility itself.",
                password_field().width(W),
            ),
            doc(
                "Email",
                "An envelope icon and a no-spaces filter. email_field() is text_field().leading(Mail).filter(…).",
                email_field().width(W),
            ),
            doc(
                "Number",
                "Accepts only digits, a decimal point and a minus sign via .filter(). Letters simply don't register.",
                number_field().placeholder("Amount").width(W),
            ),
            doc(
                "Search",
                "A leading magnifier and a clear (×) button that appears once there's text. search_field() owns the value so the button can reset it.",
                vstack(
                    children![
                        search_field().width(W).on_changed(move |s| query.set(s.to_string())),
                        muted(format!("query: {}", query.get())),
                    ],
                    10.0,
                ),
            ),
            doc(
                "Date",
                "Type digits and they auto-format to MM/DD/YYYY, or click the calendar button to pick from a month grid (navigate months with the arrows).",
                date_field().width(W),
            ),
            doc(
                "Label, helper & validation",
                "The shadcn form-field shape: a label above, helper text below, and an error state that swaps the helper for a destructive message and border. Type letters without an @ to see the error.",
                vstack(
                    children![
                        email_field()
                            .label("Email")
                            .helper("We'll never share your email.")
                            .width(W)
                            .on_changed(move |s| mail.set(s.to_string()))
                            .error_opt(mail_err),
                    ],
                    0.0,
                ),
            ),
            doc(
                "Disabled",
                "Dimmed, non-interactive and not focusable via .disabled(true).",
                text_field().label("Account ID").value("acct_9f3a1c").disabled(true).width(W),
            ),
            doc(
                "URL & phone",
                "url_field() blocks spaces; phone_field() allows digits and phone punctuation with a phone icon.",
                vstack(
                    children![url_field().width(W), gap_h(12.0), phone_field().width(W)],
                    0.0,
                ),
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
        ],
    )
}
