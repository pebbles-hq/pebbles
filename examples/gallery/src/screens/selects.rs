use pebbles::prelude::*;

use crate::ui::{doc, screen};

const PLANS: [&str; 7] = ["Free", "Pro", "Enterprise", "Team", "Startup", "Growth", "Custom"];

pub fn selects() -> Element {
    let picked = create_signal(String::from("Pro"));
    let region = create_signal(String::from("Manila"));
    let action_note = create_signal(String::new());
    let show_status = create_signal(true);
    let show_activity = create_signal(false);
    let show_panel = create_signal(true);

    screen(
        "Select & Dropdowns",
        "shadcn draws a line between a Select (pick a value) and a Dropdown Menu (run an action) — both are here. The searchable Combobox lives on its own screen. Every one opens in the overlay layer and flips up near the bottom edge.",
        children![
            doc(
                "Select — pick a value",
                "One choice from a list; the current value gets a check. This is a form control — its job is to hold a value.",
                column(
                    children![
                        select(PLANS)
                            .width(260.0)
                            .value(1)
                            .placeholder("Choose a plan")
                            .on_changed(move |_i, label| picked.set(label.to_string())),
                        muted(format!("selected: {}", picked.get())),
                    ]).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min).spacing(10.0),
            ),
            doc(
                "Select with icons",
                "Options are SelectItems, so each row can carry an icon (select_item(\"Away\").icon(…)). The trigger shows the selected item's icon, and .leading() adds a fixed one — Flutter's DropdownMenuEntry style.",
                column(
                    children![
                        select([
                            select_item("Active").icon(lucide::CIRCLE_CHECK),
                            select_item("Away").icon(lucide::CLOCK),
                            select_item("Busy").icon(lucide::CIRCLE_DOT),
                            select_item("Offline").icon(lucide::CIRCLE),
                        ])
                        .width(260.0)
                        .value(0)
                        .leading(lucide::USER)
                        .placeholder("Set status"),
                    ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min),
            ),
            doc(
                "Select — groups, disabled, clearable",
                "select_group() renders a section header; .disabled() dims an option (unpickable, keyboard skips it); .clearable(true) turns the chevron into a ✕ that resets to the placeholder.",
                column(
                    children![
                        select(
                            select_group("Asia", [
                                select_item("Manila"),
                                select_item("Tokyo"),
                                select_item("Singapore"),
                            ])
                            .into_iter()
                            .chain(select_group("Europe", [select_item("Berlin"), select_item("Paris")]))
                            .chain([select_item("Remote").disabled(true)]),
                        )
                        .width(260.0)
                        .value(0)
                        .clearable(true)
                        .placeholder("Choose a region")
                        .on_changed(move |_i, label| region.set(label.to_string()))
                        .on_cleared(move || region.set("—".into())),
                        muted(format!("selected: {}", region.get())),
                    ]).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min).spacing(10.0),
            ),
            doc(
                "Dropdown menu — run an action",
                "A menu of commands, not a value: a section label, icons, right-aligned shortcut hints, a disabled item, a separator, and a destructive action. Choosing runs it and closes.",
                column(
                    children![
                        dropdown_menu("Open menu")
                            .label("My Account")
                            .item(menu_item("Profile").icon(lucide::USER).shortcut("⇧⌘P").on_select(move || action_note.set("Profile".into())))
                            .item(menu_item("Billing").icon(lucide::CREDIT_CARD).shortcut("⌘B").on_select(move || action_note.set("Billing".into())))
                            .item(menu_item("Settings").icon(lucide::SETTINGS).shortcut("⌘,").on_select(move || action_note.set("Settings".into())))
                            .item(menu_item("Keyboard shortcuts").disabled(true))
                            .separator()
                            .item(menu_item("Log out").icon(lucide::LOG_OUT).destructive().on_select(move || action_note.set("Logged out".into()))),
                        muted(format!("last action: {}", if action_note.get().is_empty() { "—".to_string() } else { action_note.get() })),
                    ]).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min).spacing(10.0),
            ),
            doc(
                "Checkbox menu",
                "Dropdown items that toggle a boolean. (Like Radix, choosing one applies it and closes; reopen to see it checked.)",
                column(
                    children![
                        dropdown_menu("View options")
                            .label("Appearance")
                            .check("Status bar", show_status.get(), move |v| show_status.set(v))
                            .check("Activity bar", show_activity.get(), move |v| show_activity.set(v))
                            .check("Panel", show_panel.get(), move |v| show_panel.set(v)),
                        muted(format!(
                            "status: {} · activity: {} · panel: {}",
                            show_status.get(),
                            show_activity.get(),
                            show_panel.get()
                        )),
                    ]).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min).spacing(10.0),
            ),
            doc(
                "Context menu",
                "Right-click (secondary tap) opens the same menu blueprint at the cursor, clamped on-screen. Left-click elsewhere or Escape dismisses.",
                context_menu(
                    Container::new()
                        .decoration(
                            BoxDecoration::new()
                                .color(theme().colors.secondary)
                                .radius(BorderRadius::all(theme().radius)),
                        )
                        .width(280.0)
                        .height(96.0)
                        .alignment(Alignment::CENTER)
                        .child(muted("Right-click anywhere in this area")),
                )
                .item(menu_item("Cut").shortcut("⌘X"))
                .item(menu_item("Copy").shortcut("⌘C"))
                .item(menu_item("Paste").shortcut("⌘V"))
                .separator()
                .item(menu_item("Delete").destructive()),
            ),
        ],
    )
}
