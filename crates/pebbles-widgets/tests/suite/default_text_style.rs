//! DefaultTextStyle: a descendant `Text` inherits each property it didn't set,
//! explicit properties win, and nested providers compose.

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Size, palette};
use pebbles_render::{RenderParagraph, TextEnv};
use pebbles_widgets::{View, default_text_style, text};

/// Mount `root` and return the (single) resolved paragraph style.
fn paragraph_style(root: fn() -> pebbles_core::AnyWidget) -> pebbles_render::ParagraphStyle {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    ui.layout(&mut env, Size::new(300.0, 200.0));
    let tree = ui.render_tree();
    let id = tree.find::<RenderParagraph>().expect("a paragraph");
    tree.object_ref(id).downcast_ref::<RenderParagraph>().expect("paragraph").style.clone()
}

fn inherited_root() -> pebbles_core::AnyWidget {
    default_text_style(text("hi")).size(28.0).color(palette::RED).into_widget()
}

#[test]
fn plain_text_inherits_size_and_color() {
    let s = paragraph_style(inherited_root);
    assert_eq!(s.font_size, 28.0, "inherited font size");
    assert_eq!(s.color, palette::RED, "inherited color");
}

fn explicit_wins_root() -> pebbles_core::AnyWidget {
    // The Text sets its own size (11) but not color → size stays 11, color inherits.
    default_text_style(text("hi").size(11.0)).size(28.0).color(palette::RED).into_widget()
}

#[test]
fn explicit_property_wins_over_inherited() {
    let s = paragraph_style(explicit_wins_root);
    assert_eq!(s.font_size, 11.0, "the Text's explicit size wins");
    assert_eq!(s.color, palette::RED, "the unset color still inherits");
}

fn nested_root() -> pebbles_core::AnyWidget {
    // Outer sets size 28; inner sets color BLUE. The text inherits both.
    default_text_style(default_text_style(text("hi")).color(palette::BLUE)).size(28.0).into_widget()
}

#[test]
fn nested_providers_compose() {
    let s = paragraph_style(nested_root);
    assert_eq!(s.font_size, 28.0, "size from the outer provider");
    assert_eq!(s.color, palette::BLUE, "color from the inner provider");
}

fn no_provider_root() -> pebbles_core::AnyWidget {
    text("hi").size(13.0).into_widget()
}

#[test]
fn without_a_provider_text_keeps_its_own_style() {
    let s = paragraph_style(no_provider_root);
    assert_eq!(s.font_size, 13.0, "no inheritance → the Text's own style");
}

fn unstyled_root() -> pebbles_core::AnyWidget {
    text("hi").into_widget()
}

/// Regression: with no `default_text_style` above it, unstyled text used to paint in
/// `ParagraphStyle::default()` black, unreadable on a dark theme. Like Flutter's root
/// `DefaultTextStyle` (from the theme), it now takes the theme's foreground colour.
#[test]
fn unstyled_text_takes_the_theme_foreground() {
    pebbles_widgets::set_theme(pebbles_widgets::Theme::dark());
    let dark = paragraph_style(unstyled_root);
    pebbles_widgets::set_theme(pebbles_widgets::Theme::light());
    let light = paragraph_style(unstyled_root);
    assert_eq!(dark.color, pebbles_widgets::Theme::dark().colors.foreground, "dark theme foreground");
    assert_eq!(light.color, pebbles_widgets::Theme::light().colors.foreground, "light theme foreground");
}

fn provider_at_root() -> pebbles_core::AnyWidget {
    default_text_style(text("hi")).size(20.0).into_widget()
}

#[test]
fn root_provider_without_color_keeps_the_theme_foreground() {
    pebbles_widgets::set_theme(pebbles_widgets::Theme::dark());
    let s = paragraph_style(provider_at_root);
    pebbles_widgets::set_theme(pebbles_widgets::Theme::light());
    assert_eq!(s.font_size, 20.0);
    assert_eq!(s.color, pebbles_widgets::Theme::dark().colors.foreground);
}
