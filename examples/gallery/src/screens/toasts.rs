//! Toast: transient stacked notifications.

use pebbles::prelude::*;

use crate::ui::{doc, gap_w, screen};

pub fn toasts() -> Element {
    screen("Toast")
        .description(
            "Transient notifications stacked bottom-right, auto-dismissed after a few seconds or manually. Variants carry an icon + accent color; an optional action button; a title + description pair.",
        )
        .body(children![variants(), with_action(), stacked()])
}

fn variants() -> impl IntoWidget {
    doc("Variants")
        .description("Default, Success, Warning and Destructive — each brings its own icon and accent so the meaning reads at a glance.")
        .body(
            row(children![
                button("Default").variant(ButtonVariant::Outline).on_pressed(|| {
                    toast("Event created").description("Fri, Jan 3 at 5:00 PM").show();
                }),
                gap_w(10.0),
                button("Success").variant(ButtonVariant::Outline).on_pressed(|| {
                    toast("Saved").description("Your changes are live.").variant(ToastVariant::Success).show();
                }),
                gap_w(10.0),
                button("Warning").variant(ButtonVariant::Outline).on_pressed(|| {
                    toast("Heads up").description("Disk space is low.").variant(ToastVariant::Warning).show();
                }),
                gap_w(10.0),
                button("Destructive").variant(ButtonVariant::Outline).on_pressed(|| {
                    toast("Upload failed").description("The file was too large.").variant(ToastVariant::Destructive).show();
                }),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn with_action() -> impl IntoWidget {
    doc("Action & duration")
        .description(".action(label, cb) adds a tappable action (e.g. Undo); .duration(secs) holds it open longer — enough time to react.")
        .body(
            row(children![
                button("With action").on_pressed(|| {
                    toast("Message archived").action("Undo", || {}).duration(6.0).show();
                }),
                gap_w(10.0),
                button("Long-lived").variant(ButtonVariant::Outline).on_pressed(|| {
                    toast("Deploy started").description("monitoring the rollout…").duration(10.0).show();
                }),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn stacked() -> impl IntoWidget {
    doc("Stacked")
        .description(
            "Show several at once — they queue bottom-right in order and pop one by one as they expire.",
        )
        .body(
            row(children![
                button("Stack three").variant(ButtonVariant::Outline).on_pressed(|| {
                    toast("First").description("this one was first").show();
                    toast("Second").variant(ToastVariant::Success).show();
                    toast("Third").variant(ToastVariant::Warning).show();
                }),
                gap_w(10.0),
                button("Dismiss all").variant(ButtonVariant::Outline).on_pressed(|| {
                    toast("Goodbye").show();
                }),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}
