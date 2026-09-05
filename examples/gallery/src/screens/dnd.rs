//! Drag & drop / pointer control — one screen per widget: `Draggable`,
//! `DragTarget`, `LongPressDraggable`, `IgnorePointer`, `AbsorbPointer`,
//! `InteractiveViewer`, and `ReorderableListView`.

use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

// A Copy payload carried by the draggables on the Draggable / DragTarget screens.
#[derive(Clone, Copy)]
struct Fruit {
    label: &'static str,
    color: Color,
}
// A second, incompatible payload type — proves targets accept only their type.
#[derive(Clone, Copy)]
struct Tool {
    label: &'static str,
}

/// A rounded, colored chip with a white label — the thing you pick up.
fn chip(label: &str, color: Color) -> impl IntoWidget {
    Container::new()
        .color(color)
        .radius(BorderRadius::all(999.0))
        .padding(EdgeInsets { top: 8.0, bottom: 8.0, left: 16.0, right: 16.0 })
        .child(text(label).color(palette::WHITE).size(13.0).semibold())
}

/// A dashed-looking drop well that highlights when a payload hovers.
fn well(label: impl Into<String>, hovering: bool, w: f64, h: f64) -> impl IntoWidget {
    let c = theme().colors;
    let (bg, border) = if hovering { (c.primary, c.primary) } else { (c.muted, c.border) };
    Container::new()
        .width(w)
        .height(h)
        .decoration(
            BoxDecoration::new()
                .color(bg)
                .radius(BorderRadius::all(12.0))
                .border(Border::all(BorderSide::new(border, if hovering { 2.0 } else { 1.0 }))),
        )
        .child(center(text(label.into()).size(13.0).color(if hovering {
            palette::WHITE
        } else {
            c.muted_foreground
        })))
}

// ===========================================================================
// Draggable
// ===========================================================================

pub fn draggable_screen() -> Element {
    let dropped = create_signal::<Option<Fruit>>(None);
    let fruits = [
        Fruit { label: "Apple", color: palette::red::S500 },
        Fruit { label: "Banana", color: palette::amber::S500 },
        Fruit { label: "Cherry", color: palette::pink::S500 },
    ];

    screen("Draggable")
        .description("A widget you can pick up and drop onto a DragTarget; a feedback widget follows the pointer. Flutter's Draggable.")
        .body(children![
            doc("draggable(data, child).feedback(w)")
                .description("Drag a chip into the well. The chip carries a typed payload the well receives.")
                .body(column(children![
                    row(fruits
                        .iter()
                        .map(|f| {
                            let f = *f;
                            draggable(f, chip(f.label, f.color)).feedback(chip(f.label, f.color)).into_widget()
                        })
                        .collect::<Vec<_>>())
                    .main_axis_size(MainAxisSize::Min)
                    .cross_axis_alignment(CrossAxisAlignment::Center),
                    gap_h(24.0),
                    drag_target(move |hovering| {
                        let label = match dropped.get() {
                            Some(f) => format!("Dropped: {}", f.label),
                            None => "Drop a fruit here".to_string(),
                        };
                        well(label, hovering, 260.0, 96.0)
                    })
                    .on_accept::<Fruit>(move |f| dropped.set(Some(*f))),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))
        ])
}

// ===========================================================================
// DragTarget — type-filtered
// ===========================================================================

pub fn drag_target_screen() -> Element {
    let basket = create_signal(0i64);
    let toolbox = create_signal(0i64);

    screen("Drag Target")
        .description("A drop zone that receives a Draggable's payload — matched by type, so each target accepts only what it's for. Flutter's DragTarget.")
        .body(children![
            doc("drag_target(builder).on_accept::<T>(..)")
                .description("The basket accepts only fruit; the toolbox only tools. Each highlights only for a payload it will take.")
                .body(column(children![
                    row({
                        let apple = Fruit { label: "Apple", color: palette::red::S500 };
                        let wrench = Tool { label: "Wrench" };
                        children![
                            draggable(apple, chip(apple.label, apple.color))
                                .feedback(chip(apple.label, apple.color)),
                            gap_w(10.0),
                            draggable(wrench, chip(wrench.label, palette::slate::S500))
                                .feedback(chip(wrench.label, palette::slate::S500)),
                        ]
                    })
                    .main_axis_size(MainAxisSize::Min),
                    gap_h(24.0),
                    row(children![
                        drag_target(move |h| well(format!("Basket ({})", basket.get()), h, 180.0, 90.0))
                            .on_accept::<Fruit>(move |_| basket.update(|n| *n += 1)),
                        gap_w(16.0),
                        drag_target(move |h| well(format!("Toolbox ({})", toolbox.get()), h, 180.0, 90.0))
                            .on_accept::<Tool>(move |_| toolbox.update(|n| *n += 1)),
                    ])
                    .main_axis_size(MainAxisSize::Min),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))
        ])
}

// ===========================================================================
// LongPressDraggable
// ===========================================================================

pub fn long_press_draggable_screen() -> Element {
    let dropped = create_signal(false);

    screen("Long Press Draggable")
        .description("A Draggable whose drag begins only after a press-and-hold — so the child can also scroll or tap. Flutter's LongPressDraggable.")
        .body(children![
            doc("long_press_draggable(data, child)")
                .description("Press and HOLD the tile, then drag it into the well.")
                .body(column(children![
                    long_press_draggable(Fruit { label: "Hold me", color: palette::violet::S500 }, chip("Hold me", palette::violet::S500))
                        .feedback(chip("Hold me", palette::violet::S500)),
                    gap_h(24.0),
                    drag_target(move |h| {
                        well(if dropped.get() { "Dropped ✓" } else { "Drop here" }, h, 240.0, 90.0)
                    })
                    .on_accept::<Fruit>(move |_| dropped.set(true)),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))
        ])
}

// ===========================================================================
// IgnorePointer
// ===========================================================================

pub fn ignore_pointer_screen() -> Element {
    let front = create_signal(0i64);
    let behind = create_signal(0i64);
    let ignoring = create_signal(true);

    screen("Ignore Pointer")
        .description("Makes a subtree transparent to the pointer — taps fall THROUGH to whatever is behind it. Flutter's IgnorePointer.")
        .body(children![
            doc("ignore_pointer(child).enabled(on)")
                .description("With ignore ON, clicks pass through the front panel to the button behind. Toggle it off and the front panel takes the clicks itself.")
                .body(column(children![
                    button(if ignoring.get() { "ignoring: ON" } else { "ignoring: OFF" })
                        .on_pressed(move || ignoring.update(|v| *v = !*v)),
                    gap_h(16.0),
                    stack(children![
                        button(&format!("Behind ({})", behind.get()))
                            .on_pressed(move || behind.update(|n| *n += 1)),
                        ignore_pointer(
                            Container::new()
                                .width(220.0)
                                .height(52.0)
                                .decoration(
                                    BoxDecoration::new()
                                        .color(theme().colors.primary.with_alpha(0.35))
                                        .radius(BorderRadius::all(10.0)),
                                )
                                .child(center(
                                    button(&format!("Front ({})", front.get()))
                                        .on_pressed(move || front.update(|n| *n += 1)),
                                )),
                        )
                        .enabled(ignoring.get()),
                    ]),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))
        ])
}

// ===========================================================================
// AbsorbPointer
// ===========================================================================

pub fn absorb_pointer_screen() -> Element {
    let front = create_signal(0i64);
    let behind = create_signal(0i64);
    let absorbing = create_signal(true);

    screen("Absorb Pointer")
        .description("Also makes a subtree unhittable, but SWALLOWS the event so nothing behind it fires either — a lightweight modal barrier. Flutter's AbsorbPointer.")
        .body(children![
            doc("absorb_pointer(child).enabled(on)")
                .description("With absorb ON, clicks over the panel reach neither the panel's button nor the one behind it. Toggle off to restore both.")
                .body(column(children![
                    button(if absorbing.get() { "absorbing: ON" } else { "absorbing: OFF" })
                        .on_pressed(move || absorbing.update(|v| *v = !*v)),
                    gap_h(16.0),
                    stack(children![
                        button(&format!("Behind ({})", behind.get()))
                            .on_pressed(move || behind.update(|n| *n += 1)),
                        absorb_pointer(
                            Container::new()
                                .width(220.0)
                                .height(52.0)
                                .decoration(
                                    BoxDecoration::new()
                                        .color(theme().colors.destructive.with_alpha(0.30))
                                        .radius(BorderRadius::all(10.0)),
                                )
                                .child(center(
                                    button(&format!("Front ({})", front.get()))
                                        .on_pressed(move || front.update(|n| *n += 1)),
                                )),
                        )
                        .enabled(absorbing.get()),
                    ]),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))
        ])
}

// ===========================================================================
// InteractiveViewer
// ===========================================================================

pub fn interactive_viewer_screen() -> Element {
    // A colorful grid to pan/zoom around.
    let tiles: Vec<AnyWidget> = (0..24)
        .map(|i| {
            let hue = [
                palette::red::S500,
                palette::amber::S500,
                palette::green::S500,
                palette::blue::S500,
                palette::violet::S500,
                palette::pink::S500,
            ][i % 6];
            Container::new()
                .width(90.0)
                .height(90.0)
                .color(hue)
                .radius(BorderRadius::all(8.0))
                .child(center(text(&format!("{}", i + 1)).color(palette::WHITE).size(18.0).bold()))
                .into_widget()
        })
        .collect();
    let content = wrap(tiles).spacing(8.0).run_spacing(8.0);

    screen("Interactive Viewer")
        .description(
            "Pan and zoom a child within a viewport, clipped to its bounds. Flutter's InteractiveViewer.",
        )
        .body(children![
            doc("interactive_viewer(child)")
                .description("Drag to pan; double-tap to zoom in, double-tap again to reset.")
                .body(
                    Container::new()
                        .width(380.0)
                        .height(280.0)
                        .decoration(
                            BoxDecoration::new()
                                .color(theme().colors.muted)
                                .radius(BorderRadius::all(12.0))
                                .border(Border::all(BorderSide::new(theme().colors.border, 1.0))),
                        )
                        .child(
                            interactive_viewer(container().padding(EdgeInsets::all(12.0)).child(content))
                                .min_scale(0.6)
                                .max_scale(4.0),
                        )
                )
        ])
}

// ===========================================================================
// ReorderableListView
// ===========================================================================

pub fn reorderable_list_view_screen() -> Element {
    let items = create_signal(vec![
        (1u64, "Draft the proposal"),
        (2, "Review the design"),
        (3, "Ship the release"),
        (4, "Write the changelog"),
        (5, "Celebrate 🎉"),
    ]);

    screen("Reorderable List View")
        .description("A vertical list whose rows drag to reorder; each row keeps its state via a stable key. Flutter's ReorderableListView.")
        .body(children![
            doc("reorderable_list_view(rows, on_reorder)")
                .description("Drag a row up or down; the others slide to open a gap, and the order commits on drop.")
                .body({
                    let c = theme().colors;
                    let rows: Vec<(u64, AnyWidget)> = items
                        .get()
                        .iter()
                        .map(|(k, label)| {
                            let row_ui = container()
                                .padding(EdgeInsets { top: 0.0, bottom: 6.0, left: 0.0, right: 0.0 })
                                .child(
                                    Container::new()
                                        .decoration(
                                            BoxDecoration::new()
                                                .color(c.card)
                                                .radius(BorderRadius::all(10.0))
                                                .border(Border::all(BorderSide::new(c.border, 1.0))),
                                        )
                                        .padding(EdgeInsets { top: 12.0, bottom: 12.0, left: 14.0, right: 14.0 })
                                        .child(row(children![
                                            text("≡").size(16.0).color(c.muted_foreground),
                                            gap_w(12.0),
                                            text(*label).size(14.0).color(c.foreground),
                                        ])),
                                );
                            (*k, row_ui.into_widget())
                        })
                        .collect();

                    Container::new().width(360.0).child(reorderable_list_view(rows, move |from, to| {
                        items.update(|v| {
                            let it = v.remove(from);
                            v.insert(to, it);
                        })
                    })
                    .item_extent(52.0))
                })
        ])
}
