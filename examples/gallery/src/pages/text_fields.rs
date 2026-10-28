use pebbles::prelude::*;

use crate::ui::{doc, gap_h, screen};

const W: f64 = 360.0;

pub fn text_fields() -> Element {
    let name = create_signal(String::new());
    let query = create_signal(String::new());
    let mail = create_signal(String::new());
    let otp = create_signal(String::new());
    // Live validation: show an error once there's text without an "@".
    let mail_err = (!mail.get().is_empty() && !mail.get().contains('@'))
        .then(|| "Enter a valid email address".to_string());

    screen("Text Fields").body(children![
        states(),
        doc("Text — the base").body(
            column(children![
                text_field().placeholder("Your name").width(W).on_changed(move |s| name.set(s.to_string())),
                muted(format!("value: {}", name.get())),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(10.0),
        ),
        doc("Password").body(text_field().kind(InputKind::Password).width(W),),
        doc("Email").body(text_field().kind(InputKind::Email).width(W),),
        doc("Number & currency").body(
            column(children![
                text_field().kind(InputKind::Number).placeholder("Amount").width(W),
                gap_h(12.0),
                text_field().kind(InputKind::Currency).width(W),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(0.0),
        ),
        doc("Search").body(
            column(children![
                text_field().kind(InputKind::Search).width(W).on_changed(move |s| query.set(s.to_string())),
                muted(format!("query: {}", query.get())),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(10.0),
        ),
        doc("Label, helper & validation").body(
            column(children![
                text_field()
                    .kind(InputKind::Email)
                    .label("Email")
                    .helper("We'll never share your email.")
                    .width(W)
                    .on_changed(move |s| mail.set(s.to_string()))
                    .error_opt(mail_err),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(0.0),
        ),
        doc("Disabled").body(text_field().label("Account ID").value("acct_9f3a1c").disabled(true).width(W),),
        doc("URL & phone").body(
            column(children![
                text_field().kind(InputKind::Url).width(W),
                gap_h(12.0),
                text_field().kind(InputKind::Phone).width(W),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(0.0),
        ),
        doc("Character limit").body(text_field().placeholder("Max 12 chars").max_length(12).width(W),),
        doc("Multiline (textarea)").body(text_area(4).placeholder("Write a description…").width(460.0),),
        doc("Field — labeled wrapper").body(
            column(children![
                field(text_field().kind(InputKind::Email).width(W))
                    .label("Email")
                    .description("We'll never share it."),
                gap_h(16.0),
                field(text_field().width(W)).label("Username").error_opt(Some("That username is taken")),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        ),
        doc("One-time password").body(
            column(children![
                input_otp(6).group_size(3).on_changed(move |s| otp.set(s.to_string())),
                gap_h(10.0),
                row(children![
                    text(format!("value: {}", otp.get())).size(12.0).color(theme().colors.muted_foreground),
                    gap_w(10.0),
                    if otp.get().len() == 6 {
                        badge("Complete").into_widget()
                    } else {
                        gap_w(0.0).into_widget()
                    },
                ])
                .main_axis_size(MainAxisSize::Min),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        ),
    ])
}

/// The field in every state, one labelled sample each.
fn states() -> Element {
    let w = 220.0;
    doc("States").body(crate::ui::samples(vec![
        ("empty (placeholder)", text_field().placeholder("Placeholder").width(w).into_widget()),
        ("filled", text_field().value("Filled value").width(w).into_widget()),
        ("focused (autofocus)", text_field().value("Focused").autofocus().width(w).into_widget()),
        ("disabled", text_field().value("Disabled").disabled(true).width(w).into_widget()),
        ("read-only", text_field().value("Read-only").read_only(true).width(w).into_widget()),
        ("error", text_field().value("not-an-email").error("Enter a valid email.").width(w).into_widget()),
        ("with icon", text_field().placeholder("Search").leading(tabler::SEARCH).width(w).into_widget()),
        ("multi-line", text_area(3).value("Line one\nLine two").width(w).into_widget()),
    ]))
}
