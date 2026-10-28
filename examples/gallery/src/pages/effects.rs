//! Layer effects: opacity, blur (image filter and backdrop), color matrices, morphology,
//! shadows — one labelled sample each.

use pebbles::prelude::*;

use crate::ui::{doc, samples, screen};

fn art(label: &str) -> Container {
    container()
        .width(120.0)
        .height(80.0)
        .decoration(
            BoxDecoration::new()
                .gradient(Gradient::linear(
                    Alignment::TOP_LEFT,
                    Alignment::BOTTOM_RIGHT,
                    [palette::violet::S500, palette::pink::S500, palette::amber::S500],
                ))
                .radius(BorderRadius::all(10.0)),
        )
        .child(center(text(label.to_string()).bold().color(palette::WHITE)))
}

fn blur(sigma: f32) -> LayerEffect {
    LayerEffect::Blur { sigma_x: sigma, sigma_y: sigma, edge: BlurEdge::Decal }
}

const GRAYSCALE: [f32; 20] = [
    0.2126, 0.7152, 0.0722, 0.0, 0.0, //
    0.2126, 0.7152, 0.0722, 0.0, 0.0, //
    0.2126, 0.7152, 0.0722, 0.0, 0.0, //
    0.0, 0.0, 0.0, 1.0, 0.0,
];

pub fn effects_screen() -> Element {
    let stripes = || {
        row((0..8)
            .map(|i| {
                container()
                    .width(15.0)
                    .height(80.0)
                    .color(if i % 2 == 0 { palette::blue::S500 } else { palette::amber::S400 })
                    .into_widget()
            })
            .collect::<Vec<_>>())
        .main_axis_size(MainAxisSize::Min)
    };
    screen("Effects").body(children![
        doc("Opacity").body(samples(vec![
            ("1.0", opacity(1.0, art("art")).into_widget()),
            ("0.6", opacity(0.6, art("art")).into_widget()),
            ("0.2", opacity(0.2, art("art")).into_widget()),
        ])),
        doc("Image filter").body(samples(vec![
            ("none", art("art").into_widget()),
            ("blur σ2", image_filtered(blur(2.0), art("art")).into_widget()),
            ("blur σ6", image_filtered(blur(6.0), art("art")).into_widget()),
            (
                "grayscale matrix",
                image_filtered(LayerEffect::ColorMatrix(GRAYSCALE), art("art")).into_widget()
            ),
            (
                "dilate",
                image_filtered(LayerEffect::Dilate { radius_x: 2.0, radius_y: 2.0 }, art("art"))
                    .into_widget()
            ),
        ])),
        doc("Backdrop filter").body(samples(vec![
            (
                "frosted panel over stripes",
                stack(children![
                    stripes(),
                    positioned(backdrop_filter(
                        blur(4.0),
                        container().width(80.0).height(40.0).color(Color::from_rgba8(255, 255, 255, 90))
                    ))
                    .left(20.0)
                    .top(20.0),
                ])
                .into_widget(),
            ),
            ("stripes alone", stripes().into_widget()),
        ])),
        doc("Shadows").body(samples(vec![
            (
                "small",
                container()
                    .width(100.0)
                    .height(60.0)
                    .decoration(
                        BoxDecoration::new()
                            .color(theme().colors.card)
                            .radius(BorderRadius::all(8.0))
                            .shadow(BoxShadow::new(
                                Color::from_rgba8(0, 0, 0, 40),
                                Offset::new(0.0, 1.0),
                                3.0,
                                0.0
                            ),)
                    )
                    .into_widget(),
            ),
            (
                "large",
                container()
                    .width(100.0)
                    .height(60.0)
                    .decoration(
                        BoxDecoration::new()
                            .color(theme().colors.card)
                            .radius(BorderRadius::all(8.0))
                            .shadow(BoxShadow::new(
                                Color::from_rgba8(0, 0, 0, 60),
                                Offset::new(0.0, 8.0),
                                16.0,
                                -2.0
                            ),)
                    )
                    .into_widget(),
            ),
            (
                "colored, spread",
                container()
                    .width(100.0)
                    .height(60.0)
                    .decoration(
                        BoxDecoration::new()
                            .color(palette::violet::S500)
                            .radius(BorderRadius::all(8.0))
                            .shadow(BoxShadow::new(
                                Color::from_rgba8(139, 92, 246, 110),
                                Offset::new(0.0, 4.0),
                                12.0,
                                2.0
                            ),)
                    )
                    .into_widget(),
            ),
        ])),
    ])
}
