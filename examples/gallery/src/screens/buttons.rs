use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

/// A medium drop shadow (shadcn `shadow-md`).
fn shadow_md() -> BoxShadow {
    BoxShadow::new(Color::from_rgba8(0, 0, 0, 66), Offset::new(0.0, 4.0), 7.0, -1.0)
}
/// A larger, floating shadow (shadcn `shadow-lg`).
fn shadow_lg() -> BoxShadow {
    BoxShadow::new(Color::from_rgba8(0, 0, 0, 74), Offset::new(0.0, 9.0), 13.0, -3.0)
}

pub fn buttons() -> impl IntoWidget {
    let c = theme().colors;
    // Live state for the interaction sections.
    let last = create_signal(String::from("— click one —"));
    let detail = create_signal(String::from("press, then release on or off the button"));
    let lp = create_signal(String::from("hold ~0.5s, drag, or middle-click"));
    let focus_note = create_signal(String::from("(none focused)"));
    let busy = create_signal(false);
    let ev = move |name: &'static str| action(move || last.set(name.to_string()));

    screen(
        "Buttons",
        "The button component in full — every variant, size, style, state and event, so you never reach for another library.",
        children![
            // -------------------------------------------------------------- variants
            doc(
                "Variants",
                "Six built-in styles, one per emphasis level — from the solid Primary call-to-action down to the borderless Link.",
                wrap(children![
                    button("Primary"),
                    button("Secondary").variant(ButtonVariant::Secondary),
                    button("Outline").variant(ButtonVariant::Outline),
                    button("Ghost").variant(ButtonVariant::Ghost),
                    button("Destructive").variant(ButtonVariant::Destructive),
                    button("Link").variant(ButtonVariant::Link),
                ])
                .spacing(10.0),
            ),
            // ----------------------------------------------------------------- sizes
            doc(
                "Sizes",
                "Three sizes scale padding and font together, each with a minimum tap-target size so short-label buttons stay comfortable. Match the size to the density of the surface.",
                wrap(children![
                    button("Small").size(ButtonSize::Sm),
                    button("Medium").size(ButtonSize::Md),
                    button("Large").size(ButtonSize::Lg),
                ])
                .spacing(10.0),
            ),
            // --------------------------------------------------------------- icons
            doc(
                "With an icon",
                "A leading icon with .leading(), a trailing icon with .trailing(), any widget with .child(), or IconButton for a compact icon-only control.",
                column(
                    children![
                        wrap(children![
                            button("Search").leading(IconKind::Search),
                            button("Next").variant(ButtonVariant::Secondary).trailing(IconKind::ArrowRight),
                            button("Add").variant(ButtonVariant::Outline).leading(IconKind::Plus),
                            button("Menu").variant(ButtonVariant::Ghost).leading(IconKind::Menu).trailing(IconKind::ChevronDown),
                        ])
                        .spacing(10.0),
                        gap_h(12.0),
                        wrap(children![
                            icon_button(IconKind::Star),
                            icon_button(IconKind::Check).variant(ButtonVariant::Secondary),
                            icon_button(IconKind::Search).variant(ButtonVariant::Outline),
                            icon_button(IconKind::Plus).variant(ButtonVariant::Primary),
                        ])
                        .spacing(8.0),
                    ]).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min().spacing(0.0),
            ),
            // -------------------------------------------------------------- colored
            doc(
                "Colored",
                "Any brand color via .color(); pair it with .text_color() for legible contrast. Hover/press shading is derived automatically.",
                wrap(children![
                    button("Blue").color(palette::blue::S600).text_color(palette::WHITE),
                    button("Emerald").color(palette::emerald::S600).text_color(palette::WHITE),
                    button("Rose").color(palette::rose::S600).text_color(palette::WHITE),
                    button("Amber").color(palette::amber::S500).text_color(palette::zinc::S900),
                    button("Violet").color(palette::violet::S600).text_color(palette::WHITE),
                    button("Teal").color(palette::teal::S600).text_color(palette::WHITE),
                ])
                .spacing(10.0),
            ),
            // ----------------------------------------------------------- shadow/shape
            doc(
                "Shadow & shape",
                "Lift a button off the surface with .shadow() — here shadcn's medium and large elevations — or reshape it with .radius(), from sharp corners to a fully rounded pill.",
                wrap(children![
                    button("Elevated").shadow(shadow_md()),
                    button("Floating").shadow(shadow_lg()),
                    button("Colored + shadow").color(palette::indigo::S600).text_color(palette::WHITE).shadow(shadow_md()),
                    button("Pill").variant(ButtonVariant::Secondary).radius(999.0),
                    button("Sharp").variant(ButtonVariant::Outline).radius(0.0),
                ])
                .spacing(16.0),
            ),
            // ------------------------------------------------------------ full width
            doc(
                "Full width",
                "Fills the width of its container with .full_width() — ideal for forms and mobile-style layouts.",
                Container::new().width(340.0).child(button("Sign in").full_width().leading(IconKind::Check)),
            ),
            // -------------------------------------------------------------- disabled
            doc(
                "Disabled",
                "Non-interactive and dimmed via .disabled(true); the cursor becomes not-allowed. Hover and press feedback are automatic when enabled.",
                wrap(children![
                    button("Primary").disabled(true),
                    button("Secondary").variant(ButtonVariant::Secondary).disabled(true),
                    button("Outline").variant(ButtonVariant::Outline).disabled(true),
                ])
                .spacing(10.0),
            ),
            // --------------------------------------------------------------- loading
            doc(
                "Loading",
                "Show a spinner and block interaction while an async action runs with .loading(true). The label stays so the button never jumps. Toggle the switch to try it.",
                column(
                    children![
                        row(
                            children![
                                switch(busy.get()).on_changed(action(move || busy.update(|v| *v = !*v))),
                                label("Simulate busy"),
                            ]).main_axis_min().spacing(10.0),
                        gap_h(14.0),
                        wrap(children![
                            button("Save").leading(IconKind::Check).loading(busy.get()),
                            button("Please wait").variant(ButtonVariant::Secondary).loading(true),
                            button("").variant(ButtonVariant::Outline).loading(true),
                        ])
                        .spacing(12.0),
                    ]).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min().spacing(0.0),
            ),
            // ------------------------------------------------------------ click events
            doc(
                "Click events",
                "onPressed fires on a normal click; double-click, right-click (secondary) and long-press each have their own handler.",
                column(
                    children![
                        Card::new(title(format!("Last event: {}", last.get()))),
                        gap_h(12.0),
                        wrap(children![
                            button("on_pressed").on_pressed(ev("on_pressed")),
                            button("on_double_tap").variant(ButtonVariant::Secondary).on_double_tap(ev("on_double_tap")),
                            button("on_secondary_tap").variant(ButtonVariant::Outline).on_secondary_tap(ev("on_secondary_tap (right-click)")),
                            button("on_long_press").variant(ButtonVariant::Ghost).on_long_press(ev("on_long_press")),
                        ])
                        .spacing(10.0),
                    ]).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min().spacing(0.0),
            ),
            // ---------------------------------------------------------- press feedback
            doc(
                "Press feedback",
                "Follow a press precisely: the down position (on_tap_down), a cancel when you release off the button (on_tap_cancel), and highlight changes (on_highlight_changed).",
                column(
                    children![
                        Card::new(title(format!("Detail: {}", detail.get()))),
                        gap_h(12.0),
                        button("Press · drag off · release")
                            .size(ButtonSize::Lg)
                            .on_tap_down(action_event(move |e| detail.set(format!("tap-down at ({:.0}, {:.0})", e.position.x, e.position.y))))
                            .on_tap_cancel(action(move || detail.set("tap CANCELLED (released off)".into())))
                            .on_highlight_changed(move |h| { if h { detail.set("highlighted (pressed)".into()); } })
                            .on_pressed(action(move || detail.set("tapped (released inside)".into()))),
                    ]).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min().spacing(0.0),
            ),
            // -------------------------------------------------- long-press & middle-click
            doc(
                "Long-press & middle-click",
                "The full long-press lifecycle — down → start → move → end/cancel — plus middle-click (tertiary). Every payload carries the pointer position.",
                column(
                    children![
                        Card::new(title(format!("Long-press / tertiary: {}", lp.get()))),
                        gap_h(12.0),
                        button("Hold me · or middle-click")
                            .size(ButtonSize::Lg)
                            .variant(ButtonVariant::Secondary)
                            .on_long_press_down(action(move || lp.set("down (may become long-press)".into())))
                            .on_long_press_start(action_event(move |e| lp.set(format!("START at ({:.0}, {:.0})", e.position.x, e.position.y))))
                            .on_long_press_move(action_event(move |e| lp.set(format!("move ({:.0}, {:.0})", e.position.x, e.position.y))))
                            .on_long_press_end(action(move || lp.set("END (released)".into())))
                            .on_long_press_cancel(action(move || lp.set("cancelled (released too soon)".into())))
                            .on_tertiary_tap_up(action(move || lp.set("MIDDLE-click (tertiary)".into()))),
                    ]).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min().spacing(0.0),
            ),
            // ----------------------------------------------------------- focus & keyboard
            doc(
                "Focus & keyboard",
                "Tab moves focus, Enter/Space activates, .autofocus() grabs focus on mount, and .on_focus_change() reports gain/loss — a focus ring animates in.",
                column(
                    children![
                        Card::new(title(format!("Focus change: {}", focus_note.get()))),
                        gap_h(12.0),
                        wrap(children![
                            button("First (autofocus)")
                                .autofocus()
                                .on_focus_change(move |f| { if f { focus_note.set("First focused".into()); } })
                                .on_pressed(action(move || focus_note.set("First activated".into()))),
                            button("Second").variant(ButtonVariant::Secondary)
                                .on_focus_change(move |f| { if f { focus_note.set("Second focused".into()); } })
                                .on_pressed(action(move || focus_note.set("Second activated".into()))),
                            button("Third").variant(ButtonVariant::Outline)
                                .on_focus_change(move |f| { if f { focus_note.set("Third focused".into()); } })
                                .on_pressed(action(move || focus_note.set("Third activated".into()))),
                        ])
                        .spacing(10.0),
                    ]).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min().spacing(0.0),
            ),
            // ------------------------------------------------------------ custom content
            doc(
                "Custom content",
                "Put any widget inside with .child() — an icon+text row, a column, a badge, anything. The button just handles interaction and styling.",
                wrap(children![
                    button("").child(
                        row(children![
                            icon(IconKind::Star).size(16.0).color(c.primary_foreground),
                            gap_w(8.0),
                            text("Icon + Text").weight(600.0).color(c.primary_foreground),
                        ])
                        .main_axis_min(),
                    ),
                    button("").variant(ButtonVariant::Outline).child(
                        column(children![
                            text("Upgrade").weight(600.0).color(c.foreground),
                            text("Pro plan").size(11.0).color(c.muted_foreground),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .main_axis_min(),
                    ),
                ])
                .spacing(12.0),
            ),
        ],
    )
}
