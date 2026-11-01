//! Mouse cursor icons the shell hands to winit (which draws them from the system
//! cursor theme, XCURSOR_THEME / XCURSOR_SIZE on X11 and the compositor's theme on
//! Wayland): an I-beam over text fields, the theme's action cursor over buttons (the arrow in
//! the desktop Default design, the hand in Tailwind / Material), "not allowed" over a
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
fn button_shows_the_arrow_and_disabled_shows_not_allowed() {
    pebbles_widgets::set_theme(pebbles_widgets::Theme::light());
    let got = cursors_along_row(|| top_left(button("Save")), 10.0);
    assert!(got.iter().all(|c| *c == Cursor::Default), "desktop buttons keep the arrow, got {got:?}");
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

#[test]
fn web_designs_show_the_hand_over_buttons() {
    use pebbles_widgets::{DesignLanguage, Theme, set_theme};
    for d in [DesignLanguage::Material, DesignLanguage::Tailwind] {
        set_theme(Theme::light().design(d));
        assert_eq!(cursors_along_row(|| top_left(button("Save")), 10.0), [Cursor::Pointer], "{d:?}");
    }
    set_theme(Theme::light());
}

#[test]
fn a_tappable_detector_follows_the_theme_action_cursor() {
    use pebbles_widgets::{DesignLanguage, GestureDetector, Theme, set_theme};
    fn tappable() -> AnyWidget {
        top_left(GestureDetector::new(text("tap me")).on_tap(|| {}))
    }
    set_theme(Theme::light());
    assert_eq!(cursors_along_row(tappable, 8.0), [Cursor::Default]);
    set_theme(Theme::light().design(DesignLanguage::Material));
    assert_eq!(cursors_along_row(tappable, 8.0), [Cursor::Pointer]);
    set_theme(Theme::light());
}

#[test]
fn links_show_the_hand_in_the_desktop_design() {
    use pebbles_widgets::{Theme, set_theme, span, text_rich};
    set_theme(Theme::light());
    fn link() -> AnyWidget {
        top_left(
            text_rich(vec![span("go to "), span("example.com").link("https://example.com")])
                .on_link(|_| {})
                .keyboard_nav(),
        )
    }
    let got = cursors_along_row(link, 8.0);
    assert!(got.contains(&Cursor::Pointer), "{got:?}");
}
