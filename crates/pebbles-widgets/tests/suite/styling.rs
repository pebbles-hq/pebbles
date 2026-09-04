//! The universal Style system: merge precedence, `styles([..])` layering, the
//! `styled()` wrapper's box props (constraints / cursor), text props, and component
//! `.style(..)` adoption. Layout/paint asserted headlessly.

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::{BoxDecoration, Cursor, RenderConstrainedBox, RenderDecoratedBox, RenderParagraph, TextEnv};
use pebbles_widgets::{card, container, style, StyleExt, styles, text, text_field, View};

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
    ui.mount_root(View::new(palette::WHITE, component(min_sized)).into_widget());
    ui.layout(&mut env, Size::new(500.0, 500.0));
    let tree = ui.render_tree();
    let cb = tree.find::<RenderConstrainedBox>().expect("styled min → ConstrainedBox");
    let sz = tree.size_of(cb);
    assert!(sz.width >= 200.0, "min_width honored (got {})", sz.width);
    assert!(sz.height >= 60.0, "min_height honored (got {})", sz.height);
}

fn cursor_box() -> impl IntoWidget {
    container().width(120.0).height(80.0).styled(style().cursor(Cursor::Pointer))
}

#[test]
fn styled_cursor_applies() {
    pebbles_core::focus::init();
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(cursor_box)).into_widget());
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
    ui.mount_root(View::new(palette::WHITE, component(styled_text)).into_widget());
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
    ui.mount_root(View::new(palette::WHITE, component(styled_card)).into_widget());
    ui.layout(&mut env, Size::new(400.0, 300.0));
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut scene);
    // text_field().style(..) also composes without panic.
    let mut ui2 = Ui::new();
    pebbles_core::focus::init();
    pebbles_widgets::overlay::init();
    ui2.mount_root(
        View::new(palette::WHITE, component(|| text_field().style(style().background(palette::BLUE)).width(200.0))).into_widget(),
    );
    ui2.layout(&mut env, Size::new(400.0, 120.0));
    let mut scene2 = pebbles_render::Scene::new();
    ui2.paint(&mut scene2);
}

// --- §7 remaining coverage ---------------------------------------------------

fn wrapper_order_probe() -> impl IntoWidget {
    // A unique text marker wrapped with box padding(10) + margin(20). Its absolute
    // offset reveals the wrapper order: margin(outermost) + padding(innermost) = 30.
    text("x").styled(style().padding_all(10.0).background(palette::RED).margin_all(20.0))
}

#[test]
fn styled_wrapper_order_margin_outside_padding_inside() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(wrapper_order_probe)).into_widget());
    ui.layout(&mut env, Size::new(400.0, 400.0));
    let tree = ui.render_tree();
    let t = tree.find::<RenderParagraph>().expect("the text marker (unique)");
    assert_eq!(tree.absolute_offset(t), Offset::new(30.0, 30.0), "margin(20) outside, padding(10) inside");
}

fn no_op_text_style_probe() -> impl IntoWidget {
    // A text-only Style (no box props) must add NO wrapper around the widget.
    text("x").styled(style().font_size(40.0).color(palette::RED).italic(true).letter_spacing(3.0))
}

#[test]
fn text_only_style_on_box_is_a_no_op() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(no_op_text_style_probe)).into_widget());
    ui.layout(&mut env, Size::new(400.0, 400.0));
    let tree = ui.render_tree();
    let t = tree.find::<RenderParagraph>().expect("the text marker");
    // No Align/Padding/Margin wrapper was added by a text-only style → text at origin.
    assert_eq!(tree.absolute_offset(t), Offset::ZERO, "text-only style added no box wrapper");
}

#[test]
fn theme_fn_style_reevaluates_after_theme_switch() {
    use pebbles_widgets::{theme, toggle_theme};
    theme::init();
    // A style function re-reads theme() each call (never cache a Style in a static).
    let chip = || style().background(theme().colors.card);
    let before = chip().background;
    toggle_theme();
    let after = chip().background;
    assert_ne!(before, after, "the same style fn yields a new color after a theme switch");
    toggle_theme(); // restore
}

fn ellipsis_probe() -> impl IntoWidget {
    // A long string in a narrow box, clamped to 1 line with ellipsis. `center` gives
    // the child loose constraints so the paragraph reports its NATURAL (clamped) height
    // rather than being stretched to fill the window.
    pebbles_widgets::center(
        text("This is a fairly long paragraph that will not fit on one narrow line at all")
            .max_lines(1)
            .ellipsis()
            .styled(style().width(120.0)),
    )
}

#[test]
fn max_lines_ellipsis_clamps_to_one_line() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(ellipsis_probe)).into_widget());
    ui.layout(&mut env, Size::new(120.0, 200.0));
    let tree = ui.render_tree();
    let t = tree.find::<RenderParagraph>().expect("text");
    // One line tall (~ font_size * line_height ≈ 16*1.2 = 19.2), not the full wrapped height.
    assert!(tree.size_of(t).height < 30.0, "clamped to one line (got {})", tree.size_of(t).height);
}


/// A childless decorated Container fills the constraints it is given
/// (Flutter's childless `Container(color: ...)` behavior) — the primitive
/// that lets flex/stretch children and Positioned::fill render a real box.
#[test]
fn childless_decorated_container_fills() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(300.0, 200.0);
    ui.mount_root(
        View::new(
            palette::WHITE,
            pebbles_widgets::center(pebbles_widgets::widgets::SizedBox::exact(
                120.0,
                60.0,
                container().decoration(BoxDecoration::new().color(palette::BLUE)),
            )),
        )
        .into_widget(),
    );
    ui.rebuild_if_dirty();
    ui.layout(&mut env, win);
    let tree = ui.render_tree();
    let rid = tree.find::<RenderDecoratedBox>().expect("the decorated box");
    assert_eq!(tree.size_of(rid), Size::new(120.0, 60.0), "childless decorated container must fill its constraints");
}
