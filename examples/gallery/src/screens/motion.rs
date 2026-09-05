//! Motion & transitions — one screen per widget in Flutter's animation family.
//! Implicit (`Animated*`), explicit (`*Transition`, driven by a `Signal` you
//! animate), the cross-faders (`AnimatedSwitcher`/`AnimatedCrossFade`),
//! `Dismissible`, the animated collections, and `Hero`.

use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

/// A labelled demo tile at a fixed size.
fn tile(label: &str, color: Color, w: f64, h: f64) -> impl IntoWidget {
    Container::new()
        .width(w)
        .height(h)
        .color(color)
        .radius(BorderRadius::all(10.0))
        .child(center(text(label).color(palette::WHITE).size(13.0)))
}

fn play_row(a_label: &str, b_label: &str, a: impl Fn() + 'static, b: impl Fn() + 'static) -> impl IntoWidget {
    row(children![button(a_label).on_pressed(a), gap_w(8.0), button(b_label).on_pressed(b)])
        .main_axis_size(MainAxisSize::Min)
}

fn stage(control: impl IntoWidget, demo: impl IntoWidget) -> impl IntoWidget {
    column(children![control, gap_h(16.0), demo])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min)
}

// ===========================================================================
// Implicit one-property animations
// ===========================================================================

pub fn animated_opacity_screen() -> Element {
    let on = create_signal(true);
    screen("Animated Opacity")
        .description(
            "A child whose opacity tweens implicitly whenever it changes. Flutter's AnimatedOpacity.",
        )
        .body(children![
            doc("animated_opacity(value, child)")
                .description("Toggle the target and the box fades over .duration(..).")
                .body(stage(
                    button("Toggle").on_pressed(move || on.update(|v| *v = !*v)),
                    animated_opacity(
                        if on.get() { 1.0 } else { 0.15 },
                        tile("fade", theme().colors.primary, 160.0, 80.0)
                    )
                    .duration(0.4),
                ))
        ])
}

pub fn animated_scale_screen() -> Element {
    let big = create_signal(false);
    screen("Animated Scale")
        .description("A child whose scale tweens implicitly on change. Flutter's AnimatedScale.")
        .body(children![
            doc("animated_scale(factor, child)")
                .description("The box scales around its center between two factors.")
                .body(stage(
                    button("Toggle").on_pressed(move || big.update(|v| *v = !*v)),
                    center(
                        animated_scale(
                            if big.get() { 1.6 } else { 0.7 },
                            tile("scale", theme().colors.primary, 120.0, 80.0)
                        )
                        .duration(0.4)
                    ),
                ))
        ])
}

pub fn animated_rotation_screen() -> Element {
    let turns = create_signal(0.0_f64);
    screen("Animated Rotation")
        .description(
            "A child whose rotation (in turns; 1 = 360°) tweens on change. Flutter's AnimatedRotation.",
        )
        .body(children![
            doc("animated_rotation(turns, child)").description("Each press adds a quarter turn.").body(
                stage(
                    button("Rotate +90°").on_pressed(move || turns.update(|t| *t += 0.25)),
                    center(
                        animated_rotation(turns.get(), tile("rotate", theme().colors.primary, 110.0, 110.0))
                            .duration(0.4)
                    ),
                )
            )
        ])
}

pub fn animated_slide_screen() -> Element {
    let over = create_signal(false);
    screen("Animated Slide")
        .description("A child whose translation (logical px) tweens on change. Flutter's AnimatedSlide.")
        .body(children![
            doc("animated_slide(dx, dy, child)").description("The box slides between two offsets.").body(
                stage(
                    button("Toggle").on_pressed(move || over.update(|v| *v = !*v)),
                    animated_slide(
                        if over.get() { 220.0 } else { 0.0 },
                        0.0,
                        tile("slide", theme().colors.primary, 120.0, 72.0)
                    )
                    .duration(0.4),
                )
            )
        ])
}

pub fn animated_align_screen() -> Element {
    let right = create_signal(false);
    screen("Animated Align")
        .description("A child whose alignment tweens on change. Flutter's AnimatedAlign.")
        .body(children![
            doc("animated_align(alignment, child)")
                .description("The box eases between two alignments inside a fixed area.")
                .body(stage(
                    button("Toggle").on_pressed(move || right.update(|v| *v = !*v)),
                    Container::new()
                        .width(320.0)
                        .height(120.0)
                        .color(theme().colors.muted)
                        .radius(BorderRadius::all(10.0))
                        .child(
                            animated_align(
                                if right.get() { Alignment::CENTER_RIGHT } else { Alignment::CENTER_LEFT },
                                tile("align", theme().colors.primary, 72.0, 72.0)
                            )
                            .duration(0.4),
                        ),
                ))
        ])
}

pub fn animated_padding_screen() -> Element {
    let loose = create_signal(false);
    screen("Animated Padding")
        .description("A child whose padding tweens edge-by-edge on change. Flutter's AnimatedPadding.")
        .body(children![
            doc("animated_padding(insets, child)")
                .description("The inner box's inset eases between tight and loose.")
                .body(stage(
                    button("Toggle").on_pressed(move || loose.update(|v| *v = !*v)),
                    Container::new().color(theme().colors.muted).radius(BorderRadius::all(10.0)).child(
                        animated_padding(
                            if loose.get() { EdgeInsets::all(32.0) } else { EdgeInsets::all(6.0) },
                            tile("padding", theme().colors.primary, 120.0, 60.0)
                        )
                        .duration(0.4),
                    ),
                ))
        ])
}

pub fn animated_positioned_screen() -> Element {
    let far = create_signal(false);
    screen("Animated Positioned")
        .description("A Stack child whose edges/size tween on change. Flutter's AnimatedPositioned.")
        .body(children![
            doc("animated_positioned(child).left(..).top(..)")
                .description("The box eases to a new position + size within the Stack.")
                .body(stage(
                    button("Toggle").on_pressed(move || far.update(|v| *v = !*v)),
                    Container::new()
                        .width(340.0)
                        .height(180.0)
                        .color(theme().colors.muted)
                        .radius(BorderRadius::all(10.0))
                        .child(stack(children![
                            animated_positioned(tile("pos", theme().colors.primary, 0.0, 0.0))
                                .left(if far.get() { 220.0 } else { 12.0 })
                                .top(if far.get() { 108.0 } else { 12.0 })
                                .width(if far.get() { 100.0 } else { 64.0 })
                                .height(if far.get() { 56.0 } else { 64.0 })
                                .duration(0.45),
                        ]),),
                ))
        ])
}

// ===========================================================================
// Explicit transitions (driven by a Signal you animate)
// ===========================================================================

pub fn fade_transition_screen() -> Element {
    let o = create_signal(1.0_f64);
    screen("Fade Transition")
        .description("Fade a child by a Signal<f64> you animate — the Pebbles analog of Flutter's AnimationController.")
        .body(children![doc("fade_transition(opacity_signal, child)")
            .description("Drive the opacity signal with animate_to(..).")
            .body(stage(
                play_row("Fade out", "Fade in", move || animate_to(o, 0.0, 0.4), move || animate_to(o, 1.0, 0.4)),
                fade_transition(o, tile("fade", theme().colors.primary, 160.0, 80.0)),
            ))])
}

pub fn scale_transition_screen() -> Element {
    let s = create_signal(1.0_f64);
    screen("Scale Transition")
        .description("Scale a child by a Signal<f64> you animate. Flutter's ScaleTransition.")
        .body(children![doc("scale_transition(scale_signal, child)").body(stage(
            play_row("Shrink", "Grow", move || animate_to(s, 0.4, 0.4), move || animate_to(s, 1.4, 0.4)),
            center(scale_transition(s, tile("scale", theme().colors.primary, 120.0, 80.0))),
        ))])
}

pub fn rotation_transition_screen() -> Element {
    let r = create_signal(0.0_f64);
    screen("Rotation Transition")
        .description("Rotate a child by a Signal<f64> (turns) you animate. Flutter's RotationTransition.")
        .body(children![doc("rotation_transition(turns_signal, child)").body(stage(
            play_row(
                "−½ turn",
                "+½ turn",
                move || animate_to(r, r.peek() - 0.5, 0.5),
                move || animate_to(r, r.peek() + 0.5, 0.5)
            ),
            center(rotation_transition(r, tile("rotate", theme().colors.primary, 110.0, 110.0))),
        ))])
}

pub fn slide_transition_screen() -> Element {
    let t = create_signal(0.0_f64);
    let off = create_signal(Offset::ZERO);
    // Bridge the eased scalar into the Offset the transition reads.
    create_effect(move || off.set(Offset::new(t.get() * 220.0, 0.0)));
    screen("Slide Transition")
        .description("Translate a child by a Signal<Offset> you animate. Flutter's SlideTransition.")
        .body(children![
            doc("slide_transition(offset_signal, child)")
                .description("Here a scalar is eased with animate_to and mapped into the offset.")
                .body(stage(
                    play_row("Home", "Out", move || animate_to(t, 0.0, 0.4), move || animate_to(t, 1.0, 0.4)),
                    slide_transition(off, tile("slide", theme().colors.primary, 120.0, 72.0)),
                ))
        ])
}

pub fn size_transition_screen() -> Element {
    let f = create_signal(1.0_f64);
    screen("Size Transition")
        .description("Reveal a child along an axis by a Signal<f64> factor (clip + height/width factor). Flutter's SizeTransition.")
        .body(children![doc("size_transition(factor_signal, child)")
            .description("The box reveals/collapses vertically.")
            .body(stage(
                play_row("Collapse", "Reveal", move || animate_to(f, 0.0, 0.4), move || animate_to(f, 1.0, 0.4)),
                size_transition(f, tile("size", theme().colors.primary, 200.0, 90.0)),
            ))])
}

pub fn positioned_transition_screen() -> Element {
    let t = create_signal(0.0_f64);
    let rect = create_signal(Rect::new(12.0, 12.0, 76.0, 76.0));
    create_effect(move || {
        let p = t.get();
        let x = 12.0 + p * 208.0;
        let y = 12.0 + p * 96.0;
        rect.set(Rect::new(x, y, x + 100.0, y + 56.0));
    });
    screen("Positioned Transition")
        .description(
            "Position + size a Stack child by a Signal<Rect> you animate. Flutter's PositionedTransition.",
        )
        .body(children![
            doc("positioned_transition(rect_signal, child)").body(stage(
                play_row(
                    "Corner",
                    "Move",
                    move || animate_to(t, 0.0, 0.45),
                    move || animate_to(t, 1.0, 0.45)
                ),
                Container::new()
                    .width(340.0)
                    .height(180.0)
                    .color(theme().colors.muted)
                    .radius(BorderRadius::all(10.0))
                    .child(stack(children![positioned_transition(
                        rect,
                        tile("rect", theme().colors.primary, 0.0, 0.0)
                    )]),),
            ))
        ])
}

pub fn decorated_box_transition_screen() -> Element {
    let t = create_signal(0.0_f64);
    let from = BoxDecoration::new().color(palette::blue::S500).radius(BorderRadius::all(6.0));
    let to = BoxDecoration::new()
        .color(palette::red::S500)
        .radius(BorderRadius::all(40.0))
        .border(Border::all(BorderSide::new(palette::WHITE, 3.0)));
    screen("Decorated Box Transition")
        .description("Cross-fade a child's BoxDecoration (color, radius, border, shadows) by a Signal<f64>. Flutter's DecoratedBoxTransition.")
        .body(children![doc("decorated_box_transition(from, to, t_signal, child)")
            .body(stage(
                play_row("A", "B", move || animate_to(t, 0.0, 0.5), move || animate_to(t, 1.0, 0.5)),
                center(decorated_box_transition(from.clone(), to.clone(), t, SizedBox::exact(140.0, 140.0, center(text("decoration").color(palette::WHITE).size(12.0))))),
            ))])
}

// ===========================================================================
// Cross-faders
// ===========================================================================

pub fn animated_switcher_screen() -> Element {
    let i = create_signal(0usize);
    const COLORS: [Color; 3] = [palette::blue::S500, palette::green::S500, palette::amber::S500];
    const NAMES: [&str; 3] = ["one", "two", "three"];
    let k = i.get() as u64;
    screen("Animated Switcher")
        .description("Cross-fade to a new child whenever its key changes. Flutter's AnimatedSwitcher.")
        .body(children![
            doc("animated_switcher(key, child)")
                .description("Each press changes the key → the old child fades out while the new fades in.")
                .body(stage(
                    button("Next").on_pressed(move || i.update(|n| *n = (*n + 1) % 3)),
                    center(
                        animated_switcher(k, tile(NAMES[i.get() % 3], COLORS[i.get() % 3], 160.0, 90.0))
                            .duration(0.35)
                    ),
                ))
        ])
}

pub fn animated_cross_fade_screen() -> Element {
    let second = create_signal(false);
    screen("Animated Cross Fade")
        .description("Cross-fade between two fixed children on a bool. Flutter's AnimatedCrossFade.")
        .body(children![
            doc("animated_cross_fade(first, second, show_second)").body(stage(
                button("Toggle").on_pressed(move || second.update(|v| *v = !*v)),
                center(
                    animated_cross_fade(
                        tile("first", palette::blue::S500, 160.0, 90.0),
                        tile("second", palette::violet::S500, 160.0, 90.0),
                        second.get(),
                    )
                    .duration(0.4)
                ),
            ))
        ])
}

// ===========================================================================
// Dismissible + animated collections
// ===========================================================================

pub fn dismissible_screen() -> Element {
    let items = create_signal(vec![1u64, 2, 3, 4, 5]);
    let rows: Vec<AnyWidget> = items
        .get()
        .iter()
        .map(|&id| {
            let items = items;
            dismissible(
                Container::new()
                    .width(320.0)
                    .height(52.0)
                    .color(theme().colors.secondary)
                    .radius(BorderRadius::all(8.0))
                    .child(padding(
                        EdgeInsets::symmetric(14.0, 0.0),
                        row(children![
                            text(format!("Item {id} — swipe me")).color(theme().colors.foreground)
                        ])
                        .main_axis_size(MainAxisSize::Min),
                    )),
                move || items.update(|v| v.retain(|x| *x != id)),
            )
            .into_widget()
        })
        .collect();
    screen("Dismissible")
        .description("Swipe a child horizontally past a threshold to dismiss it — it slides off, fades, then fires on_dismissed. Flutter's Dismissible.")
        .body(children![doc("dismissible(child, on_dismissed)")
            .description("Drag a row sideways to remove it.")
            .body(column(rows).main_axis_size(MainAxisSize::Min))])
}

pub fn animated_list_screen() -> Element {
    let items = create_signal(vec![1u64, 2, 3]);
    let next = create_signal(4u64);
    let list_items: Vec<(u64, AnyWidget)> = items
        .get()
        .iter()
        .map(|&id| {
            (
                id,
                Container::new()
                    .width(300.0)
                    .height(46.0)
                    .color(theme().colors.secondary)
                    .radius(BorderRadius::all(8.0))
                    .margin(EdgeInsets { top: 0.0, right: 0.0, bottom: 6.0, left: 0.0 })
                    .child(padding(
                        EdgeInsets::symmetric(12.0, 0.0),
                        row(children![text(format!("Row {id}")).color(theme().colors.foreground)])
                            .main_axis_size(MainAxisSize::Min),
                    ))
                    .into_widget(),
            )
        })
        .collect();
    screen("Animated List")
        .description("Items animate in on add and out on remove; a removed item is held for one exit tween, then dropped. Flutter's AnimatedList.")
        .body(children![doc("animated_list((key, child) pairs)")
            .description("Add appends a new row (enters); Remove drops the first (exits).")
            .body(stage(
                play_row(
                    "Add",
                    "Remove first",
                    move || { let id = next.get(); next.update(|n| *n += 1); items.update(move |v| v.push(id)); },
                    move || items.update(|v| { if !v.is_empty() { v.remove(0); } }),
                ),
                animated_list(list_items).duration(0.3),
            ))])
}

pub fn animated_grid_screen() -> Element {
    let items = create_signal(vec![1u64, 2, 3, 4, 5, 6]);
    let next = create_signal(7u64);
    let cells: Vec<(u64, AnyWidget)> = items
        .get()
        .iter()
        .map(|&id| (id, tile(&format!("{id}"), theme().colors.primary, 72.0, 72.0).into_widget()))
        .collect();
    screen("Animated Grid")
        .description(
            "A flowing grid whose tiles scale + fade in on add and out on remove. Flutter's AnimatedGrid.",
        )
        .body(children![
            doc("animated_grid((key, child) pairs)")
                .description("Add a tile (scales in) or remove the last (scales out).")
                .body(stage(
                    play_row(
                        "Add",
                        "Remove last",
                        move || {
                            let id = next.get();
                            next.update(|n| *n += 1);
                            items.update(move |v| v.push(id));
                        },
                        move || items.update(|v| {
                            v.pop();
                        }),
                    ),
                    animated_grid(cells).spacing(10.0).duration(0.3),
                ))
        ])
}

// ===========================================================================
// Hero
// ===========================================================================

pub fn hero_screen() -> Element {
    let detail = create_signal(false);
    let body: AnyWidget = if detail.get() {
        column(children![
            button("← Back").on_pressed(move || fly_heroes(0.4, move || detail.set(false))),
            gap_h(16.0),
            center(hero("hero-demo", tile("HERO", theme().colors.primary, 220.0, 220.0))),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
    } else {
        column(children![
            button("Open detail →").on_pressed(move || fly_heroes(0.4, move || detail.set(true))),
            gap_h(16.0),
            hero("hero-demo", tile("hero", theme().colors.primary, 84.0, 84.0)),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
    };
    screen("Hero")
        .description("Shared-element transition: the tagged widget flies from its old rect to its new one across a navigation. Flutter's Hero. Here the 'grid' and 'detail' views share hero(\"hero-demo\").")
        .body(children![doc("hero(tag, child) + fly_heroes(dur, navigate)")
            .description("Open/Back run the route change through fly_heroes, so the box flies between the two layouts.")
            .body(body)])
}
