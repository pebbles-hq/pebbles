//! Raw input — Flutter's `Listener` + `Focus.onKeyEvent` on Pebbles: pointer signals
//! (wheel / trackpad), any-button down/up, local hover position, raw key events for a
//! focused widget, a physical-key shortcut, and canvas text measured in monospace cells.

use std::rc::Rc;

use pebbles::prelude::*;

use crate::ui::{doc, screen};

const MONO: &str = "JetBrains Mono";

pub fn listener_screen() -> Element {
    let colors = theme().colors;

    // ----- pointer: wheel, buttons, hover -----
    let wheel = create_signal(0.0_f64);
    let hover = create_signal(String::from("—"));
    let button = create_signal(String::from("—"));
    let pad = GestureDetector::new(
        container().width(340.0).height(120.0).color(colors.muted).padding(EdgeInsets::all(12.0)).child(
            column(children![
                text(format!("wheel total: {:.0} px (consumed)", wheel.get())),
                text(format!("hover (local): {}", hover.get())),
                text(format!("last button: {}", button.get())),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        ),
    )
    .on_pointer_signal(move |e| {
        wheel.update(|w| *w += e.delta.y);
        true // consume: the page behind does not scroll
    })
    .on_pointer_hover(action_event(move |e| {
        hover.set(format!("{:.0}, {:.0}", e.position.x, e.position.y));
    }))
    .on_button_down(action_event(move |e| {
        button.set(format!("{:?} down (buttons {})", e.button, e.buttons))
    }));

    // ----- keyboard: raw events for a focused widget -----
    let node = create_focus();
    let last_key = create_signal(String::from("click here, then press PageUp / F5 / Insert …"));
    node.register(Rc::new(|| {}), None, false);
    node.on_key_event(Rc::new(move |e| {
        if !e.is_press() {
            return KeyEventResult::Ignored;
        }
        last_key.set(format!("{:?} {:?} ({})", e.logical, e.kind, e.physical.code()));
        match e.logical {
            // Claim paging keys; everything else keeps its normal meaning (Tab still moves focus).
            LogicalKey::Named(NamedKey::PageUp | NamedKey::PageDown | NamedKey::Insert) => {
                KeyEventResult::Handled
            }
            _ => KeyEventResult::Ignored,
        }
    }));
    let focused = node.is_focused();
    let key_box = GestureDetector::new(
        container()
            .width(340.0)
            .padding(EdgeInsets::all(12.0))
            .color(if focused { colors.accent } else { colors.muted })
            .child(text(last_key.get())),
    )
    .on_tap(move || node.request_focus());

    // ----- shortcuts: a physical binding works on every layout -----
    let toggles = create_signal(0_u32);
    create_shortcut(Shortcut::physical("Ctrl+Shift+`"), move || toggles.update(|t| *t += 1));

    // ----- canvas text on a monospace grid -----
    let fg = colors.foreground;
    let grid = canvas(move |c: &mut Canvas<'_>| {
        let cell = c.monospace_cell(MONO, 13.0, 1.3);
        let style = pebbles::render::ParagraphStyle {
            font_size: 13.0,
            line_height: 1.3,
            font_family: Some(MONO.into()),
            soft_wrap: false,
            color: fg,
            ..Default::default()
        };
        for (row, line) in ["$ ls -la", "drwxr-xr-x  src", "-rw-r--r--  Cargo.toml"].iter().enumerate() {
            for (col, ch) in line.chars().enumerate() {
                let at = Offset::new(col as f64 * cell.width, row as f64 * cell.height);
                c.draw_text(&ch.to_string(), &style, at);
            }
        }
    })
    .width(340.0)
    .height(60.0);

    screen("Listener & Raw Keys").body(children![
        doc("Pointer signal, buttons, hover").body(pad),
        doc("Focus.onKeyEvent").body(key_box),
        doc("Physical shortcut")
            .body(text(format!("Ctrl+Shift+` pressed {} times (any layout)", toggles.get()))),
        doc("Canvas text in monospace cells").body(grid),
    ])
}
