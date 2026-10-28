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

pub fn buttons() -> Element {
    let c = theme().colors;
    // Live state for the interaction sections.
    let last = create_signal(String::from("— click one —"));
    let detail = create_signal(String::from("press, then release on or off the button"));
    let lp = create_signal(String::from("hold ~0.5s, drag, or middle-click"));
    let focus_note = create_signal(String::from("(none focused)"));
    let busy = create_signal(false);
    let ev = move |name: &'static str| move || last.set(name.to_string());

    screen("Buttons").body(children![
        states(),
        // -------------------------------------------------------------- variants
        doc("Variants").body(
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
        doc("Sizes").body(
            wrap(children![
                button("Small").size(ButtonSize::Sm),
                button("Medium").size(ButtonSize::Md),
                button("Large").size(ButtonSize::Lg),
            ])
            .spacing(10.0),
        ),
        // --------------------------------------------------------------- icons
        doc("With an icon").body(
            column(children![
                wrap(children![
                    button("Search").leading(IconKind::Search),
                    button("Next").variant(ButtonVariant::Secondary).trailing(IconKind::ArrowRight),
                    button("Add").variant(ButtonVariant::Outline).leading(IconKind::Plus),
                    button("Menu")
                        .variant(ButtonVariant::Ghost)
                        .leading(IconKind::Menu)
                        .trailing(IconKind::ChevronDown),
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
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(0.0),
        ),
        // -------------------------------------------------------------- colored
        doc("Colored").body(
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
        doc("Shadow & shape").body(
            wrap(children![
                button("Elevated").shadow(shadow_md()),
                button("Floating").shadow(shadow_lg()),
                button("Colored + shadow")
                    .color(palette::indigo::S600)
                    .text_color(palette::WHITE)
                    .shadow(shadow_md()),
                button("Pill").variant(ButtonVariant::Secondary).radius(999.0),
                button("Sharp").variant(ButtonVariant::Outline).radius(0.0),
            ])
            .spacing(16.0),
        ),
        // ------------------------------------------------------------ full width
        doc("Full width")
            .body(container().width(340.0).child(button("Sign in").full_width().leading(IconKind::Check)),),
        // -------------------------------------------------------------- disabled
        doc("Disabled").body(
            wrap(children![
                button("Primary").disabled(true),
                button("Secondary").variant(ButtonVariant::Secondary).disabled(true),
                button("Outline").variant(ButtonVariant::Outline).disabled(true),
            ])
            .spacing(10.0),
        ),
        // --------------------------------------------------------------- loading
        doc("Loading").body(
            column(children![
                row(children![
                    switch(busy.get()).on_changed(move || busy.update(|v| *v = !*v)),
                    label("Simulate busy"),
                ])
                .main_axis_size(MainAxisSize::Min)
                .spacing(10.0),
                gap_h(14.0),
                wrap(children![
                    button("Save").leading(IconKind::Check).loading(busy.get()),
                    button("Please wait").variant(ButtonVariant::Secondary).loading(true),
                    button("").variant(ButtonVariant::Outline).loading(true),
                ])
                .spacing(12.0),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(0.0),
        ),
        // ------------------------------------------------------------ click events
        doc("Click events").body(
            column(children![
                card().child(title(format!("Last event: {}", last.get()))),
                gap_h(12.0),
                wrap(children![
                    button("on_pressed").on_pressed(ev("on_pressed")),
                    button("on_double_tap")
                        .variant(ButtonVariant::Secondary)
                        .on_double_tap(ev("on_double_tap")),
                    button("on_secondary_tap")
                        .variant(ButtonVariant::Outline)
                        .on_secondary_tap(ev("on_secondary_tap (right-click)")),
                    button("on_long_press").variant(ButtonVariant::Ghost).on_long_press(ev("on_long_press")),
                ])
                .spacing(10.0),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(0.0),
        ),
        // ---------------------------------------------------------- press feedback
        doc("Press feedback").body(
            column(children![
                card().child(title(format!("Detail: {}", detail.get()))),
                gap_h(12.0),
                button("Press · drag off · release")
                    .size(ButtonSize::Lg)
                    .on_tap_down(action_event(move |e| detail
                        .set(format!("tap-down at ({:.0}, {:.0})", e.position.x, e.position.y))))
                    .on_tap_cancel(move || detail.set("tap CANCELLED (released off)".into()))
                    .on_highlight_changed(move |h| {
                        if h {
                            detail.set("highlighted (pressed)".into());
                        }
                    })
                    .on_pressed(move || detail.set("tapped (released inside)".into())),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(0.0),
        ),
        // -------------------------------------------------- long-press & middle-click
        doc("Long-press & middle-click").body(
            column(children![
                card().child(title(format!("Long-press / tertiary: {}", lp.get()))),
                gap_h(12.0),
                button("Hold me · or middle-click")
                    .size(ButtonSize::Lg)
                    .variant(ButtonVariant::Secondary)
                    .on_long_press_down(move || lp.set("down (may become long-press)".into()))
                    .on_long_press_start(action_event(
                        move |e| lp.set(format!("START at ({:.0}, {:.0})", e.position.x, e.position.y))
                    ))
                    .on_long_press_move(action_event(
                        move |e| lp.set(format!("move ({:.0}, {:.0})", e.position.x, e.position.y))
                    ))
                    .on_long_press_end(move || lp.set("END (released)".into()))
                    .on_long_press_cancel(move || lp.set("cancelled (released too soon)".into()))
                    .on_tertiary_tap_up(move || lp.set("MIDDLE-click (tertiary)".into())),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(0.0),
        ),
        // ----------------------------------------------------------- focus & keyboard
        doc("Focus & keyboard").body(
            column(children![
                card().child(title(format!("Focus change: {}", focus_note.get()))),
                gap_h(12.0),
                wrap(children![
                    button("First")
                        .on_focus_change(move |f| {
                            if f {
                                focus_note.set("First focused".into());
                            }
                        })
                        .on_pressed(move || focus_note.set("First activated".into())),
                    button("Second")
                        .variant(ButtonVariant::Secondary)
                        .on_focus_change(move |f| {
                            if f {
                                focus_note.set("Second focused".into());
                            }
                        })
                        .on_pressed(move || focus_note.set("Second activated".into())),
                    button("Third")
                        .variant(ButtonVariant::Outline)
                        .on_focus_change(move |f| {
                            if f {
                                focus_note.set("Third focused".into());
                            }
                        })
                        .on_pressed(move || focus_note.set("Third activated".into())),
                ])
                .spacing(10.0),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(0.0),
        ),
        // ------------------------------------------------------------ custom content
        doc("Custom content").body(
            wrap(children![
                button("").child(
                    row(children![
                        icon(IconKind::Star).size(16.0).color(c.primary_foreground),
                        gap_w(8.0),
                        text("Icon + Text").weight(600.0).color(c.primary_foreground),
                    ])
                    .main_axis_size(MainAxisSize::Min),
                ),
                button("").variant(ButtonVariant::Outline).child(
                    column(children![
                        text("Upgrade").weight(600.0).color(c.foreground),
                        text("Pro plan").size(11.0).color(c.muted_foreground),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Center)
                    .main_axis_size(MainAxisSize::Min),
                ),
            ])
            .spacing(12.0),
        ),
    ])
}

/// Every variant in every showable state, one labelled sample each.
fn states() -> Element {
    let variants = [
        ("Primary", ButtonVariant::Primary),
        ("Secondary", ButtonVariant::Secondary),
        ("Outline", ButtonVariant::Outline),
        ("Ghost", ButtonVariant::Ghost),
        ("Destructive", ButtonVariant::Destructive),
        ("Link", ButtonVariant::Link),
    ];
    let rows: Vec<AnyWidget> = variants
        .into_iter()
        .map(|(name, v)| {
            let mut items = vec![("normal", button(name).variant(v).into_widget())];
            // One autofocused button shows the focus ring (only one node can hold focus).
            if v == ButtonVariant::Primary {
                items.push(("focused (autofocus)", button(name).variant(v).autofocus().into_widget()));
            }
            items.extend([
                ("disabled", button(name).variant(v).disabled(true).into_widget()),
                ("loading", button(name).variant(v).loading(true).into_widget()),
                ("with icon", button(name).variant(v).leading(IconKind::Plus).into_widget()),
                ("icon only", icon_button(IconKind::Plus).variant(v).label(name).into_widget()),
                (
                    "icon only, disabled",
                    icon_button(IconKind::Plus).variant(v).disabled(true).label(name).into_widget(),
                ),
            ]);
            crate::ui::samples(items).into_widget()
        })
        .collect();
    doc("States").body(column(rows).spacing(12.0).cross_axis_alignment(CrossAxisAlignment::Start))
}
