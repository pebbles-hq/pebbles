//! The universal Style system: merge precedence, `styles([..])` layering, the
//! `styled()` wrapper's box props (constraints / cursor), text props, and component
//! `.style(..)` adoption. Layout/paint asserted headlessly.

use pebbles_core::{IntoWidget, Ui, WidgetExt, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::{Cursor, RenderConstrainedBox, TextEnv};
use pebbles_widgets::{Container, StyleExt, View, card, style, styles, text, text_field};

#[test]
fn merge_precedence_and_shadow_replacement() {
    use pebbles_render::BoxShadow;
    let base = style().background(palette::RED).width(100.0).shadow(BoxShadow::new(
        palette::BLACK,
        Offset::new(0.0, 1.0),
        2.0,
        0.0,
    ));
    let over = style().background(palette::BLUE); // only background set
    let m = base.clone().merge(over);
    assert_eq!(m.background, Some(palette::BLUE), "override wins per-field");
    assert_eq!(m.width, Some(100.0), "unset field falls through");
    assert_eq!(m.shadows.len(), 1, "base shadow retained when override has none");

    // A non-empty shadow list in the override replaces wholesale.
    let over2 = style().shadow(BoxShadow::new(palette::WHITE, Offset::new(0.0, 4.0), 8.0, 0.0));
    let m2 = base.merge(over2);
    assert_eq!(m2.shadows.len(), 1);
    assert_eq!(m2.shadows[0].blur, 8.0, "override shadow replaced the base one");
}

#[test]
fn styles_layers_left_to_right() {
    let s = styles([
        style().background(palette::RED).width(50.0),
        style().background(palette::GREEN),
        style().width(80.0).height(20.0),
    ]);
    assert_eq!(s.background, Some(palette::GREEN), "middle layer's bg wins over first");
    assert_eq!(s.width, Some(80.0), "last layer's width wins");
    assert_eq!(s.height, Some(20.0));
}

fn min_sized() -> impl IntoWidget {
    text("x").styled(style().min_width(200.0).min_height(60.0))
}

#[test]
fn styled_min_constraints_apply() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(min_sized)).boxed());
    ui.layout(&mut env, Size::new(500.0, 500.0));
    let tree = ui.render_tree();
    let cb = tree.find::<RenderConstrainedBox>().expect("styled min → ConstrainedBox");
    let sz = tree.size_of(cb);
    assert!(sz.width >= 200.0, "min_width honored (got {})", sz.width);
    assert!(sz.height >= 60.0, "min_height honored (got {})", sz.height);
}

fn cursor_box() -> impl IntoWidget {
    Container::new().width(120.0).height(80.0).styled(style().cursor(Cursor::Pointer))
}

#[test]
fn styled_cursor_applies() {
    pebbles_core::focus::init();
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(cursor_box)).boxed());
    ui.layout(&mut env, Size::new(300.0, 300.0));
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut scene);
    assert_eq!(ui.cursor_at(Offset::new(40.0, 30.0)), Some(Cursor::Pointer), "style cursor wraps a GestureDetector");
}

fn styled_text() -> impl IntoWidget {
    // Text props exercised through Style (compile + paint smoke): italic, underline,
    // letter spacing, alignment, max_lines.
    text("styled text sample that is fairly long to allow wrapping").style(
        style()
            .italic(true)
            .underline(true)
            .letter_spacing(0.5)
            .text_align(pebbles_foundation::TextAlign::Center)
            .max_lines(2)
            .font_size(14.0),
    )
}

#[test]
fn text_style_props_lay_out_and_paint() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(styled_text)).boxed());
    ui.layout(&mut env, Size::new(160.0, 200.0)); // narrow → wraps
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut scene); // must not panic with the parley style props set
}

fn styled_card() -> impl IntoWidget {
    // Component adoption: override the card's radius/background via .style().
    card().child(text("body")).style(style().background(palette::RED).radius_all(0.0))
}

#[test]
fn component_style_adoption_builds_and_paints() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(styled_card)).boxed());
    ui.layout(&mut env, Size::new(400.0, 300.0));
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut scene);
    // text_field().style(..) also composes without panic.
    let mut ui2 = Ui::new();
    pebbles_core::focus::init();
    pebbles_widgets::overlay::init();
    ui2.mount_root(
        View::new(palette::WHITE, component(|| text_field().style(style().background(palette::BLUE)).width(200.0))).boxed(),
    );
    ui2.layout(&mut env, Size::new(400.0, 120.0));
    let mut scene2 = pebbles_render::Scene::new();
    ui2.paint(&mut scene2);
}
