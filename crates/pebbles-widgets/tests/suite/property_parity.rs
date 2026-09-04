//! Property-parity batch (p2 §G): `Flexible::fit` (G4), `Text::soft_wrap` (G5),
//! and `Container::foreground_decoration` (G8) — small, additive Flutter properties.

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{FlexFit, Size, palette};
use pebbles_render::{Border, BorderRadius, BoxDecoration, RenderParagraph, TextEnv};
use pebbles_widgets::{SizedBox, View, column, container, flexible, row, text};

#[test]
fn flexible_fit_tight_forces_the_child_to_fill_its_share() {
    let mut tight = Ui::new();
    let mut env = TextEnv::new();
    tight.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                SizedBox::exact(
                    200.0,
                    40.0,
                    row(vec![flexible(text("hi".to_string())).flex(1).fit(FlexFit::Tight).into_widget()]),
                )
            }),
        )
        .into_widget(),
    );
    tight.layout(&mut env, Size::new(300.0, 100.0));
    let w_tight = tight
        .render_tree()
        .find::<RenderParagraph>()
        .map(|id| tight.render_tree().size_of(id).width)
        .unwrap();

    let mut loose = Ui::new();
    let mut env2 = TextEnv::new();
    loose.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                SizedBox::exact(
                    200.0,
                    40.0,
                    row(vec![flexible(text("hi".to_string())).flex(1).fit(FlexFit::Loose).into_widget()]),
                )
            }),
        )
        .into_widget(),
    );
    loose.layout(&mut env2, Size::new(300.0, 100.0));
    let w_loose = loose
        .render_tree()
        .find::<RenderParagraph>()
        .map(|id| loose.render_tree().size_of(id).width)
        .unwrap();

    assert!(w_tight > w_loose, "tight fills its share ({w_tight}) > loose natural ({w_loose})");
}

#[test]
fn text_soft_wrap_false_shapes_a_single_unbroken_line() {
    const LONG: &str = "this is a long sentence that would normally wrap across several lines";

    let mut wrapped = Ui::new();
    let mut env = TextEnv::new();
    wrapped.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![
                    pebbles_widgets::constrained_box(
                        pebbles_render::BoxConstraints::loose(Size::new(120.0, f64::INFINITY)),
                        text(LONG.to_string()),
                    )
                    .into_widget(),
                ])
            }),
        )
        .into_widget(),
    );
    wrapped.layout(&mut env, Size::new(200.0, 300.0));
    let h_wrapped = wrapped
        .render_tree()
        .find::<RenderParagraph>()
        .map(|id| wrapped.render_tree().size_of(id).height)
        .unwrap();

    let mut nowrap = Ui::new();
    let mut env2 = TextEnv::new();
    nowrap.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![
                    pebbles_widgets::constrained_box(
                        pebbles_render::BoxConstraints::loose(Size::new(120.0, f64::INFINITY)),
                        text(LONG.to_string()).soft_wrap(false),
                    )
                    .into_widget(),
                ])
            }),
        )
        .into_widget(),
    );
    nowrap.layout(&mut env2, Size::new(200.0, 300.0));
    let h_nowrap = nowrap
        .render_tree()
        .find::<RenderParagraph>()
        .map(|id| nowrap.render_tree().size_of(id).height)
        .unwrap();

    assert!(
        h_nowrap < h_wrapped,
        "soft_wrap(false) is one line ({h_nowrap}), wrapped is taller ({h_wrapped})"
    );
}

#[test]
fn foreground_decoration_paints_over_the_child_without_panic() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                SizedBox::exact(
                    120.0,
                    80.0,
                    container()
                        .color(palette::BLUE)
                        .radius(BorderRadius::all(12.0))
                        .foreground_decoration(
                            BoxDecoration::new()
                                .border(Border::new(palette::RED, 2.0))
                                .radius(BorderRadius::all(12.0)),
                        )
                        .child(text("over".to_string())),
                )
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, Size::new(200.0, 160.0));
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut env, &mut scene); // encodes the foreground border — must not panic
}
