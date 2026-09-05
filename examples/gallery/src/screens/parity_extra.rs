//! Buildable-now Flutter-parity widgets: scroll notifications, `ListBody`,
//! the selection-control list tiles, `DraggableScrollableSheet`, the Scaffold
//! drawer / persistent-bottom-sheet slots, and `DefaultTextStyle`.

use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

// ===========================================================================
// Mobile runtime: PopScope + SystemChrome
// ===========================================================================

pub fn mobile_runtime_screen() -> Element {
    let blocking = create_signal(true);
    let last = create_signal(String::from("—"));
    let dark_icons = create_signal(false);

    screen("Mobile Runtime")
        .description("Shell-driven hooks, built as reactive desktop-testable APIs (the mobile shell drives them on-device): PopScope (Android back button), SystemChrome (status/nav-bar styling), and Scaffold.resize_to_avoid_bottom_inset (soft keyboard).")
        .body(children![
            doc("pop_scope(child).blocking(..).on_pop(..)")
                .description("Intercept the hardware back button. Toggle blocking, then simulate a back press — the shell calls dispatch_back().")
                .body(pop_scope(column(children![
                    row(children![
                        switch(blocking.get()).on_changed(move || blocking.update(|v| *v = !*v)),
                        gap_w(10.0),
                        text(if blocking.get() { "Blocking (back is intercepted)" } else { "Transparent (system pops)" }),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Center),
                    gap_h(12.0),
                    button("Simulate back press").on_pressed(move || {
                        let consumed = dispatch_back();
                        last.set(if consumed { "consumed by pop_scope".into() } else { "no handler — would exit".into() });
                    }),
                    gap_h(8.0),
                    text(format!("Result: {}", last.get())).color(theme().colors.muted_foreground),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))
                .blocking(blocking.get())
                .on_pop(|| {})),
            doc("set_system_ui_overlay_style(..)")
                .description("Request status/navigation-bar styling (a no-op on desktop; the mobile shell applies it). The requested value round-trips through system_ui_overlay_style().")
                .body(column(children![
                    row(children![
                        switch(dark_icons.get()).on_changed(move || {
                            dark_icons.update(|v| *v = !*v);
                            set_system_ui_overlay_style(SystemUiOverlayStyle {
                                status_bar_dark_icons: dark_icons.peek(),
                                ..Default::default()
                            });
                        }),
                        gap_w(10.0),
                        text("Dark status-bar icons"),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Center),
                    gap_h(8.0),
                    text(format!("requested dark icons = {}", system_ui_overlay_style().status_bar_dark_icons))
                        .color(theme().colors.muted_foreground),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min)),
            doc("scaffold(..).resize_to_avoid_bottom_inset(true)")
                .description("The Scaffold lifts its shell above the soft keyboard by MediaQuery.view_insets.bottom (reported by the mobile shell via set_view_insets). On desktop the insets are zero, so it's a no-op — not simulated here to avoid insetting the gallery's own shell.")
                .body(text("Driven by the mobile shell's keyboard-height reports.").color(theme().colors.muted_foreground)),
        ])
}

// ===========================================================================
// DefaultTextStyle
// ===========================================================================

pub fn default_text_style_screen() -> Element {
    let animated = create_signal(false);
    let c = theme().colors;

    screen("Default Text Style")
        .description("Flutter's DefaultTextStyle: set an ambient text style for a subtree. Descendant Text widgets inherit each property they didn't set explicitly; nested providers compose. animated_default_text_style eases the transition.")
        .body(children![
            doc("default_text_style(child).size(..).color(..)")
                .description("All three lines inherit size + color from the provider; the middle one overrides the weight, the last overrides the color.")
                .body(default_text_style(column(children![
                    text("Inherits size and color"),
                    text("Overrides weight only").bold(),
                    text("Overrides color only").color(c.primary),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))
                .size(18.0)
                .color(c.muted_foreground)),
            doc("animated_default_text_style(child).duration(..)")
                .description("Toggle to ease the whole subtree between two styles (size + color + weight).")
                .body(column(children![
                    button(if animated.get() { "Shrink / mute" } else { "Grow / emphasize" })
                        .on_pressed(move || animated.update(|v| *v = !*v)),
                    gap_h(14.0),
                    animated_default_text_style(column(children![
                        text("The quick brown fox"),
                        text("jumps over the lazy dog"),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min))
                    .duration(0.25)
                    .size(if animated.get() { 26.0 } else { 15.0 })
                    .weight(if animated.get() { 700.0 } else { 400.0 })
                    .color(if animated.get() { c.primary } else { c.muted_foreground }),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min)),
        ])
}

/// A bordered, clipped frame around a fixed-size demo area.
fn framed(w: f64, h: f64, child: impl IntoWidget) -> impl IntoWidget {
    let c = theme().colors;
    SizedBox::exact(
        w,
        h,
        container()
            .decoration(
                BoxDecoration::new().border(Border::new(c.border, 1.0)).radius(BorderRadius::all(12.0)),
            )
            .clip()
            .child(child),
    )
}

// ===========================================================================
// ScrollNotification — .on_scroll
// ===========================================================================

pub fn scroll_notification_screen() -> Element {
    let pixels = create_signal(0.0_f64);
    let frac = create_signal(0.0_f64);

    let mut rows: Vec<AnyWidget> = Vec::new();
    for i in 0..40 {
        rows.push(
            container()
                .padding(EdgeInsets::symmetric(12.0, 10.0))
                .child(text(format!("Row {i}")))
                .into_widget(),
        );
    }
    let list = scroll_view(column(rows).main_axis_size(MainAxisSize::Min)).on_scroll(move |n| {
        pixels.set(n.metrics.pixels);
        frac.set(n.metrics.fraction());
    });

    screen("Scroll Notification")
        .description("Pebbles' direct equivalent of Flutter's NotificationListener<ScrollNotification>: a scroll view's .on_scroll callback fires with live metrics (offset, extent, fraction) plus Start/Update/End/Overscroll events.")
        .body(children![
            doc("scroll_view(..).on_scroll(|n| ..)")
                .description("Scroll the list — the readout tracks the live offset and progress.")
                .body(column(children![
                    row(children![
                        text("offset:").weight(600.0),
                        gap_w(6.0),
                        text(format!("{:.0} px", pixels.get())),
                        gap_w(20.0),
                        text("progress:").weight(600.0),
                        gap_w(6.0),
                        text(format!("{:.0}%", frac.get() * 100.0)),
                    ]),
                    gap_h(12.0),
                    framed(280.0, 220.0, list),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))
        ])
}

// ===========================================================================
// ListBody
// ===========================================================================

pub fn list_body_screen() -> Element {
    let make = |label: &str| {
        container()
            .decoration(BoxDecoration::new().color(theme().colors.secondary).radius(BorderRadius::all(8.0)))
            .padding(EdgeInsets::all(10.0))
            .child(text(label.to_string()))
    };
    screen("List Body")
        .description("Flutter's ListBody: lays children out sequentially along an axis, each at its natural extent and stretched on the cross axis — the non-scrolling body you drop inside a scroll view. It's a Column/Row sized to the sum of its children.")
        .body(children![
            doc("list_body(children)")
                .description("Vertical by default; .horizontal() lays them in a row.")
                .body(column(children![
                    container().width(260.0).child(list_body(children![
                        make("First"),
                        make("Second"),
                        make("Third"),
                    ])),
                    gap_h(16.0),
                    list_body(children![make("A"), make("B"), make("C")]).horizontal(),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))
        ])
}

// ===========================================================================
// Selection-control list tiles
// ===========================================================================

pub fn list_tiles_screen() -> Element {
    let wifi = create_signal(true);
    let bluetooth = create_signal(false);
    let plan = create_signal(0_usize);

    screen("List Tiles")
        .description("CheckboxListTile / SwitchListTile / RadioListTile — a ListTile with a trailing selection control where the whole row is the tap target (Flutter parity).")
        .body(children![
            doc("checkbox_list_tile / switch_list_tile")
                .description("Tapping anywhere on the row toggles the control.")
                .body(container().width(340.0).child(column(children![
                    checkbox_list_tile("Enable notifications", wifi.get())
                        .subtitle("Push, email and SMS")
                        .on_changed(move || wifi.update(|v| *v = !*v)),
                    switch_list_tile("Bluetooth", bluetooth.get())
                        .on_changed(move || bluetooth.update(|v| *v = !*v)),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_size(MainAxisSize::Min))),
            doc("radio_list_tile")
                .description("A single-select group of rows.")
                .body(container().width(340.0).child(column(
                    ["Free", "Pro", "Team"]
                        .iter()
                        .enumerate()
                        .map(|(i, label)| {
                            radio_list_tile(*label, plan.get() == i)
                                .on_changed(move || plan.set(i))
                                .into_widget()
                        })
                        .collect::<Vec<_>>(),
                )
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_size(MainAxisSize::Min))),
        ])
}

// ===========================================================================
// DraggableScrollableSheet
// ===========================================================================

pub fn draggable_sheet_screen() -> Element {
    let mut rows: Vec<AnyWidget> = Vec::new();
    for i in 0..30 {
        rows.push(
            container()
                .padding(EdgeInsets::symmetric(16.0, 12.0))
                .child(text(format!("Item {i}")))
                .into_widget(),
        );
    }
    let sheet = draggable_scrollable_sheet(column(rows).main_axis_size(MainAxisSize::Min))
        .initial(0.4)
        .min(0.2)
        .max(0.95)
        .snap([0.2, 0.4, 0.95]);

    screen("Draggable Scrollable Sheet")
        .description("Flutter's DraggableScrollableSheet: a bottom-anchored panel sized as a fraction of the available space. Drag the top handle to resize (snapping to stops); the body scrolls when it overflows.")
        .body(children![
            doc("draggable_scrollable_sheet(content).initial/min/max/snap")
                .description("Drag the grab handle up and down; release to snap to 20% / 40% / 95%.")
                .body(framed(320.0, 380.0, sheet))
        ])
}

// ===========================================================================
// Scaffold drawer + persistent bottom sheet
// ===========================================================================

pub fn scaffold_drawer_screen() -> Element {
    let shell = scaffold(center(text("Body content")))
        .top(top_panel("My App").leading(drawer_button()))
        .drawer(
            column(children![
                text("Menu").size(16.0).weight(700.0),
                gap_h(12.0),
                list_tile("Home").on_tap(|| {}),
                list_tile("Settings").on_tap(|| {}),
                list_tile("About").on_tap(|| {}),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min),
        )
        .bottom_sheet(
            row(children![
                text("3 items selected").weight(500.0),
                spacer(),
                button("Delete").variant(ButtonVariant::Destructive),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Center),
        );

    screen("Scaffold: Drawer & Bottom Sheet")
        .description("Scaffold.drawer (opened by the hamburger drawer_button, sliding in as a sheet) and Scaffold.bottom_sheet (a persistent, non-modal panel pinned above the bottom bar).")
        .body(children![
            doc("scaffold(..).drawer(..).bottom_sheet(..)")
                .description("Tap the hamburger to open the drawer. The bottom panel is always present.")
                .body(framed(420.0, 320.0, shell))
        ])
}
