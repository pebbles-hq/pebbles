use pebbles::prelude::*;

use crate::ui::{doc, gap_h, screen};

pub fn toggles() -> Element {
    screen(
        "Toggles",
        "Selection controls — checkbox, switch, radio and toggle — each with sizes, colors, labels, disabled and focus states, animated on change.",
        children![checkbox_section(), switch_section(), radio_section(), toggle_section()],
    )
}

// ---------------------------------------------------------------------------
// Checkbox
// ---------------------------------------------------------------------------

fn checkbox_section() -> impl IntoWidget {
    let terms = create_signal(true);
    let notify = create_signal(false);

    doc(
        "Checkbox",
        "A binary choice. Bind it to a signal and flip it in on_changed; add a label — or a label plus description — and the whole row becomes the target.",
        column(children![
            // interactive, labeled
            column(children![
                checkbox(terms.get())
                    .label("Accept terms and conditions")
                    .on_changed(move || terms.update(|v| *v = !*v)),
                gap_h(12.0),
                checkbox(notify.get())
                    .label("Email notifications")
                    .description("Get notified about product updates and account activity.")
                    .on_changed(move || notify.update(|v| *v = !*v)),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_min(),
            gap_h(20.0),
            // sizes
            wrap(children![
                checkbox(true).size(ToggleSize::Sm).label("Small"),
                checkbox(true).size(ToggleSize::Md).label("Medium"),
                checkbox(true).size(ToggleSize::Lg).label("Large"),
            ])
            .spacing(24.0),
            gap_h(18.0),
            // colors
            wrap(children![
                checkbox(true).color(palette::emerald::S600).label("Emerald"),
                checkbox(true).color(palette::blue::S600).label("Blue"),
                checkbox(true).color(palette::rose::S600).label("Rose"),
                checkbox(true).color(palette::violet::S600).label("Violet"),
            ])
            .spacing(24.0),
            gap_h(18.0),
            // disabled
            wrap(children![
                checkbox(true).disabled(true).label("Checked · disabled"),
                checkbox(false).disabled(true).label("Unchecked · disabled"),
            ])
            .spacing(24.0),
            gap_h(18.0),
            // indeterminate (mixed) — a filled box with a dash
            wrap(children![
                checkbox(false).indeterminate(true).label("Indeterminate"),
                checkbox(true).indeterminate(true).color(palette::emerald::S600).label("Indeterminate · colored"),
            ])
            .spacing(24.0),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_min(),
    )
}

// ---------------------------------------------------------------------------
// Switch
// ---------------------------------------------------------------------------

fn switch_section() -> impl IntoWidget {
    let airplane = create_signal(true);
    let wifi = create_signal(false);

    doc(
        "Switch",
        "An instant on/off toggle for settings that apply immediately. The thumb slides and the track fades between states.",
        column(children![
            column(children![
                switch(airplane.get())
                    .label("Airplane mode")
                    .on_changed(move || airplane.update(|v| *v = !*v)),
                gap_h(12.0),
                switch(wifi.get())
                    .label("Wi-Fi")
                    .description("Connect automatically to known networks.")
                    .on_changed(move || wifi.update(|v| *v = !*v)),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_min(),
            gap_h(20.0),
            // sizes
            wrap(children![
                switch(true).size(ToggleSize::Sm).label("Small"),
                switch(true).size(ToggleSize::Md).label("Medium"),
                switch(true).size(ToggleSize::Lg).label("Large"),
            ])
            .spacing(24.0),
            gap_h(18.0),
            // colors
            wrap(children![
                switch(true).color(palette::emerald::S600).label("Emerald"),
                switch(true).color(palette::blue::S600).label("Blue"),
                switch(true).color(palette::amber::S500).label("Amber"),
            ])
            .spacing(24.0),
            gap_h(18.0),
            // disabled
            wrap(children![
                switch(true).disabled(true).label("On · disabled"),
                switch(false).disabled(true).label("Off · disabled"),
            ])
            .spacing(24.0),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_min(),
    )
}

// ---------------------------------------------------------------------------
// Radio
// ---------------------------------------------------------------------------

fn radio_section() -> impl IntoWidget {
    let plan = create_signal(1usize);

    let plans: Vec<_> = ["Free", "Pro", "Enterprise"]
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            radio(plan.get() == i).label(name).on_selected(move || plan.set(i)).into_widget()
        })
        .collect();

    doc(
        "Radio",
        "A single choice from a set — one selected at a time. Give each option the same on_selected that stores its index; the selected index drives which is filled.",
        column(children![
            // interactive group
            column(plans).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min().spacing(12.0),
            gap_h(20.0),
            // sizes
            wrap(children![
                radio(true).size(ToggleSize::Sm).label("Small"),
                radio(true).size(ToggleSize::Md).label("Medium"),
                radio(true).size(ToggleSize::Lg).label("Large"),
            ])
            .spacing(24.0),
            gap_h(18.0),
            // colors
            wrap(children![
                radio(true).color(palette::emerald::S600).label("Emerald"),
                radio(true).color(palette::blue::S600).label("Blue"),
                radio(true).color(palette::rose::S600).label("Rose"),
            ])
            .spacing(24.0),
            gap_h(18.0),
            // disabled
            wrap(children![
                radio(true).disabled(true).label("Selected · disabled"),
                radio(false).disabled(true).label("Unselected · disabled"),
            ])
            .spacing(24.0),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_min(),
    )
}

// ---------------------------------------------------------------------------
// Toggle
// ---------------------------------------------------------------------------

fn toggle_section() -> impl IntoWidget {
    let favorite = create_signal(true);
    let visible = create_signal(true);
    let locked = create_signal(false);

    doc(
        "Toggle",
        "A button that stays pressed — wrap any child (an icon or short text). Two variants, three sizes, and a filled active state.",
        column(children![
            // a small icon toolbar of independent toggles
            row(children![
                toggle(favorite.get(), icon(IconKind::Star).size(16.0))
                    .on_changed(move || favorite.update(|v| *v = !*v)),
                toggle(visible.get(), icon(IconKind::Eye).size(16.0))
                    .on_changed(move || visible.update(|v| *v = !*v)),
                toggle(locked.get(), icon(IconKind::Lock).size(16.0))
                    .on_changed(move || locked.update(|v| *v = !*v)),
            ])
            .main_axis_min()
            .spacing(8.0),
            gap_h(20.0),
            // variants
            wrap(children![
                toggle(true, text("Default")),
                toggle(true, text("Outline")).variant(ToggleVariant::Outline),
                toggle(false, text("Inactive")).variant(ToggleVariant::Outline),
            ])
            .spacing(10.0),
            gap_h(18.0),
            // sizes
            wrap(children![
                toggle(true, text("Small")).size(ToggleSize::Sm).variant(ToggleVariant::Outline),
                toggle(true, text("Medium")).size(ToggleSize::Md).variant(ToggleVariant::Outline),
                toggle(true, text("Large")).size(ToggleSize::Lg).variant(ToggleVariant::Outline),
            ])
            .spacing(10.0),
            gap_h(18.0),
            // colors + disabled
            wrap(children![
                toggle(true, icon(IconKind::Star).size(16.0)).color(palette::amber::S500),
                toggle(true, icon(IconKind::Check).size(16.0)).color(palette::emerald::S600),
                toggle(true, text("Disabled")).variant(ToggleVariant::Outline).disabled(true),
            ])
            .spacing(10.0),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_min(),
    )
}
