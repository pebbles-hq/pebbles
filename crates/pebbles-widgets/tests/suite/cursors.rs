//! Mouse cursor icons the shell hands to winit (which draws them from the system
//! cursor theme, XCURSOR_THEME / XCURSOR_SIZE on X11 and the compositor's theme on
//! Wayland): an I-beam over text fields, a hand over buttons, "not allowed" over a
//! disabled button, resize arrows over a resizable panel handle.

use pebbles_core::widget::AnyWidget;
use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{CrossAxisAlignment, Offset, Size, palette};
use pebbles_render::{Cursor, TextEnv};
use pebbles_widgets::{OverlayHost, View, button, column, resizable, text, text_field};

fn cursors_along_row(root: fn() -> AnyWidget, y: f64) -> Vec<Cursor> {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    for _ in 0..2 {
        ui.rebuild_if_dirty();
        ui.layout(&mut env, Size::new(800.0, 400.0));
        ui.publish_bounds();
    }
    let mut seen = Vec::new();
    for x in (0..800).step_by(2) {
        if let Some(c) = ui.cursor_at(Offset::new(f64::from(x), y))
            && !seen.contains(&c)
        {
            seen.push(c);
        }
    }
    seen
}

fn top_left(w: impl IntoWidget) -> AnyWidget {
    OverlayHost::wrap(column(vec![w.into_widget()]).cross_axis_alignment(CrossAxisAlignment::Start))
        .into_widget()
}

#[test]
fn text_field_shows_the_i_beam() {
    let got = cursors_along_row(|| top_left(text_field().width(240.0)), 10.0);
    assert_eq!(got, [Cursor::Text]);
}

#[test]
fn button_shows_the_hand_and_disabled_shows_not_allowed() {
    assert_eq!(cursors_along_row(|| top_left(button("Save")), 10.0), [Cursor::Pointer]);
    assert_eq!(cursors_along_row(|| top_left(button("Save").disabled(true)), 10.0), [Cursor::NotAllowed]);
}

#[test]
fn resizable_handle_shows_the_resize_arrow() {
    let got = cursors_along_row(
        || top_left(resizable(vec![text("left").into_widget(), text("right").into_widget()])),
        10.0,
    );
    assert!(got.contains(&Cursor::ColResize), "a horizontal handle is a col-resize target, got {got:?}");
}
