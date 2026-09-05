//! Painting & effects — one screen per widget: `ClipRect`, `ClipOval`, `ClipPath`,
//! `ColorFiltered`, and `ShaderMask`. (Pebbles already ships `Canvas`/`CustomPaint`,
//! `Opacity`, `ClipRRect`, and `RepaintBoundary`.)

use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

/// A gradient-filled art swatch with a centered label — the thing we clip/tint/mask.
fn art(w: f64, h: f64) -> impl IntoWidget {
    Container::new()
        .width(w)
        .height(h)
        .decoration(BoxDecoration::new().gradient(Gradient::linear(
            Alignment::TOP_LEFT,
            Alignment::BOTTOM_RIGHT,
            [palette::violet::S500, palette::pink::S500, palette::amber::S500],
        )))
        .child(center(text("art").color(palette::WHITE).size(16.0).bold()))
}

fn labeled(label: &str, child: impl IntoWidget) -> impl IntoWidget {
    column(children![text(label).size(12.0).color(theme().colors.muted_foreground), gap_h(8.0), child,])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min)
}

// ===========================================================================
// ClipRect
// ===========================================================================

pub fn clip_rect_screen() -> Element {
    screen("Clip Rect")
        .description("Clips a child to a plain rectangle (its bounds). Flutter's ClipRect. (A thin convenience over the existing ClipRRect with a zero radius.)")
        .body(children![
            doc("clip_rect(child)")
                .description("A 200×120 art swatch overflows a 140×90 slot on the right; clip_rect trims it to the slot.")
                .body(row(children![
                    labeled("unclipped 140×90 slot", SizedBox::exact(140.0, 90.0, art(200.0, 120.0))),
                    gap_w(24.0),
                    labeled("clip_rect", SizedBox::exact(140.0, 90.0, clip_rect(art(200.0, 120.0)))),
                ])
                .main_axis_size(MainAxisSize::Min))
        ])
}

// ===========================================================================
// ClipOval
// ===========================================================================

pub fn clip_oval_screen() -> Element {
    screen("Clip Oval")
        .description("Clips a child to the ellipse (a circle for a square box) inscribed in its bounds. Flutter's ClipOval.")
        .body(children![
            doc("clip_oval(child)")
                .description("The classic circular avatar: a square art swatch clipped to a circle.")
                .body(row(children![
                    labeled("square", SizedBox::exact(120.0, 120.0, art(120.0, 120.0))),
                    gap_w(24.0),
                    labeled("clip_oval (circle)", clip_oval(SizedBox::exact(120.0, 120.0, art(120.0, 120.0)))),
                    gap_w(24.0),
                    labeled("clip_oval (ellipse)", clip_oval(SizedBox::exact(180.0, 100.0, art(180.0, 100.0)))),
                ])
                .main_axis_size(MainAxisSize::Min))
        ])
}

// ===========================================================================
// ClipPath
// ===========================================================================

pub fn clip_path_screen() -> Element {
    screen("Clip Path")
        .description("Clips a child to a path your delegate builds from the box size. Flutter's ClipPath + CustomClipper.")
        .body(children![
            doc("clip_path(|size| BezPath, child)")
                .description("A triangle and a chevron clip, each built from the box size.")
                .body(row(children![
                    labeled(
                        "triangle",
                        clip_path(
                            |s: Size| {
                                let mut p = BezPath::new();
                                p.move_to((s.width / 2.0, 0.0));
                                p.line_to((s.width, s.height));
                                p.line_to((0.0, s.height));
                                p.close_path();
                                p
                            },
                            SizedBox::exact(140.0, 130.0, art(140.0, 130.0)),
                        ),
                    ),
                    gap_w(24.0),
                    labeled(
                        "chevron",
                        clip_path(
                            |s: Size| {
                                let notch = s.width * 0.22;
                                let mut p = BezPath::new();
                                p.move_to((0.0, 0.0));
                                p.line_to((s.width - notch, 0.0));
                                p.line_to((s.width, s.height / 2.0));
                                p.line_to((s.width - notch, s.height));
                                p.line_to((0.0, s.height));
                                p.close_path();
                                p
                            },
                            SizedBox::exact(180.0, 90.0, art(180.0, 90.0)),
                        ),
                    ),
                ])
                .main_axis_size(MainAxisSize::Min))
        ])
}

// ===========================================================================
// ColorFiltered
// ===========================================================================

pub fn color_filtered_screen() -> Element {
    screen("Color Filtered")
        .description(
            "Blends a color over a child with a blend mode. Flutter's ColorFiltered (ColorFilter.mode).",
        )
        .body(children![
            doc("color_filtered(color, child).blend(mode)")
                .description("The same art multiplied by a teal tint, and screened with a warm color.")
                .body(
                    row(children![
                        labeled("original", SizedBox::exact(150.0, 110.0, art(150.0, 110.0))),
                        gap_w(20.0),
                        labeled(
                            "multiply (teal)",
                            color_filtered(
                                palette::teal::S400,
                                SizedBox::exact(150.0, 110.0, art(150.0, 110.0))
                            ),
                        ),
                        gap_w(20.0),
                        labeled(
                            "screen (amber)",
                            color_filtered(
                                palette::amber::S300,
                                SizedBox::exact(150.0, 110.0, art(150.0, 110.0))
                            )
                            .blend(BlendMode::Screen),
                        ),
                    ])
                    .main_axis_size(MainAxisSize::Min)
                )
        ])
}

// ===========================================================================
// ShaderMask
// ===========================================================================

pub fn shader_mask_screen() -> Element {
    screen("Shader Mask")
        .description("Masks a child by a gradient's luminance — bright areas keep the child, dark areas hide it. Flutter's ShaderMask (the common fade/vignette).")
        .body(children![
            doc("shader_mask(gradient, child)")
                .description("A vertical white→black gradient fades the art out toward the bottom; a radial one vignettes it.")
                .body(row(children![
                    labeled("original", SizedBox::exact(160.0, 130.0, art(160.0, 130.0))),
                    gap_w(20.0),
                    labeled(
                        "fade (linear)",
                        shader_mask(
                            Gradient::vertical([palette::WHITE, palette::BLACK]),
                            SizedBox::exact(160.0, 130.0, art(160.0, 130.0)),
                        ),
                    ),
                    gap_w(20.0),
                    labeled(
                        "vignette (radial)",
                        shader_mask(
                            Gradient::radial(0.75, [palette::WHITE, palette::BLACK]),
                            SizedBox::exact(160.0, 130.0, art(160.0, 130.0)),
                        ),
                    ),
                ])
                .main_axis_size(MainAxisSize::Min))
        ])
}
