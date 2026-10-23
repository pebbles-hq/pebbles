//! screen/ — deterministic, in-repo stand-ins for real app screens (§7.2, V25). Category
//! Screen, slice Screens; procedural images only, no sibling crates, no network.

use pebbles_core::IntoWidget as _;
use pebbles_foundation::{Alignment, Color, EdgeInsets, Offset};
use pebbles_icons::tabler;
use pebbles_render::paint::{Painter, kurbo, peniko};
use pebbles_render::{BorderRadius, BoxShadow, Gradient, image_from_rgba8};
use pebbles_testing::Harness;
use pebbles_widgets::{
    Transform, backdrop_filter, button, canvas, checkbox, chip, column, container, expanded, icon,
    icon_button, list_tile, list_view, positioned, radio, row, separator, slider, span, stack, switch, table,
    text, text_field, text_rich, widget_span, wrap,
};

use kurbo::{Affine, BezPath, Point, Rect, RoundedRect, Stroke};
use peniko::{BlendMode, Compose, Fill, Mix};

use crate::case::{Case, Category::Screen, Slice::Screens, harness, ops};

pub fn cases() -> Vec<Case> {
    let mut v = vec![ops("screen/dashboard_1200x800", Screen, Screens, 1200, 800, dashboard)];
    v.extend([1.5, 2.0].map(|s| {
        let mut c = ops("screen/dashboard_1200x800", Screen, Screens, 1200, 800, dashboard);
        c.scale = s;
        c.name = format!("screen/dashboard_1200x800_s{}", (s * 100.0) as u32);
        c.in_refs = false; // refs only at 1.0
        c
    }));
    v.extend([
        harness("screen/app_shell", Screen, Screens, 1200, 800, app_shell),
        harness("screen/landing_like", Screen, Screens, 1200, 860, landing_like),
        harness("screen/charts_like", Screen, Screens, 900, 600, charts_like),
        harness("screen/code_editor_like", Screen, Screens, 900, 600, code_editor_like),
        harness("screen/markdown_like", Screen, Screens, 900, 1000, markdown_like),
        harness("screen/form", Screen, Screens, 600, 700, form),
        // V53 step 3: Flutter-parity screens — DR-24 features used the way a Flutter app uses them.
        harness("screen/flutter_parity_frosted_app_bar", Screen, Screens, 900, 700, frosted_app_bar),
        harness("screen/flutter_parity_card_stack", Screen, Screens, 900, 600, card_stack),
        harness("screen/flutter_parity_article", Screen, Screens, 800, 1000, article),
    ]);
    v
}

// ---- dashboard: Build::Ops port of spikes/renderer-bench (main.rs:525-640) -------------------

const DW: f64 = 1200.0;

fn rgb(r: u8, g: u8, b: u8) -> peniko::Color {
    peniko::Color::from_rgb8(r, g, b)
}

fn linear(p0: Point, p1: Point, stops: [peniko::Color; 2]) -> peniko::Gradient {
    peniko::Gradient::new_linear(p0, p1).with_stops(stops.as_slice())
}

pub(crate) fn dashboard(p: &mut Painter<'_>) {
    let id = Affine::IDENTITY;
    p.fill(Fill::NonZero, id, rgb(15, 17, 26), None, &Rect::new(0.0, 0.0, DW, 800.0));
    let hero = linear(Point::ZERO, Point::new(DW, 220.0), [rgb(99, 102, 241), rgb(236, 72, 153)]);
    p.fill(Fill::NonZero, id, &peniko::Brush::Gradient(hero), None, &Rect::new(0.0, 0.0, DW, 220.0));
    let accent = || linear(Point::ZERO, Point::new(DW, 0.0), [rgb(16, 185, 129), rgb(59, 130, 246)]);
    let (cols, rows, mx, my, gap, ch) = (3.0, 2, 48.0, 260.0, 32.0, 150.0);
    let cw = (DW - 2.0 * mx - (cols - 1.0) * gap) / cols;
    for r in 0..rows {
        for c in 0..3 {
            let x = mx + f64::from(c) * (cw + gap);
            let y = my + f64::from(r) * (ch + gap);
            let card = Rect::new(x, y, x + cw, y + ch);
            let rr = RoundedRect::from_rect(card, 16.0);
            p.draw_blurred_rounded_rect(id, card, peniko::Color::from_rgba8(0, 0, 0, 90), 16.0, 14.0);
            p.fill(Fill::NonZero, id, rgb(28, 31, 44), None, &rr);
            p.push_layer(Fill::NonZero, Mix::Normal, 1.0, id, &rr);
            let header = Rect::new(card.x0, card.y0, card.x1, card.y0 + 40.0);
            p.fill(Fill::NonZero, id, &peniko::Brush::Gradient(accent()), None, &header);
            p.pop_layer();
            p.stroke(&Stroke::new(1.0), id, peniko::Color::from_rgba8(255, 255, 255, 24), None, &rr);
        }
    }
    // ShaderMask (DestIn): a magenta panel faded out to the right.
    let mb = Rect::new(48.0, 620.0, 560.0, 680.0);
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, id, &mb);
    p.fill(Fill::NonZero, id, rgb(236, 72, 153), None, &mb);
    p.push_layer(Fill::NonZero, BlendMode::new(Mix::Normal, Compose::DestIn), 1.0, id, &mb);
    let fade = linear(
        Point::new(mb.x0, 0.0),
        Point::new(mb.x1, 0.0),
        [peniko::Color::from_rgba8(255, 255, 255, 255), peniko::Color::from_rgba8(255, 255, 255, 0)],
    );
    p.fill(Fill::NonZero, id, &peniko::Brush::Gradient(fade), None, &mb);
    p.pop_layer();
    p.pop_layer();
    // Translucent bottom action bar.
    p.push_layer(Fill::NonZero, Mix::Normal, 0.85, id, &Rect::new(0.0, 0.0, DW, 800.0));
    p.fill(
        Fill::NonZero,
        id,
        &peniko::Brush::Gradient(accent()),
        None,
        &Rect::new(48.0, 700.0, DW - 48.0, 760.0),
    );
    p.pop_layer();
}

// ---- widget screens ---------------------------------------------------------------------------

/// A deterministic `w`×`h` procedural image (diagonal bands tinted by `seed`).
fn procedural(w: u32, h: u32, seed: u32) -> pebbles_render::Image {
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let band = ((x + y + seed * 17) / 12) % 4;
            let t = (band * 50 + seed * 23) as u8;
            px.extend_from_slice(&[
                t.wrapping_add(80),
                (x * 255 / w.max(1)) as u8,
                (y * 255 / h.max(1)) as u8,
                255,
            ]);
        }
    }
    image_from_rgba8(w, h, px)
}

/// A `w`×`h` box showing `img` (a decoration image: no `image-view` codecs or HTTP client).
fn picture(img: pebbles_render::Image, w: f64, h: f64) -> pebbles_widgets::Container {
    container().width(w).height(h).decoration(pebbles_render::BoxDecoration::new().image(img))
}

fn shadow() -> BoxShadow {
    BoxShadow::new(Color::from_rgba8(0, 0, 0, 40), Offset::new(0.0, 4.0), 16.0, 0.0)
}

fn app_shell(h: &mut Harness) {
    let sidebar = column(
        (0..8)
            .map(|i| list_tile(format!("Section {i}")).leading(icon(tabler::HOME)).into_widget())
            .collect::<Vec<_>>(),
    );
    let toolbar = row(vec![
        icon_button(tabler::MENU_2).into_widget(),
        icon_button(tabler::SEARCH).into_widget(),
        icon_button(tabler::BELL).into_widget(),
        icon_button(tabler::SETTINGS).into_widget(),
        expanded(text_field().placeholder("Search…")).into_widget(),
    ])
    .spacing(8.0);
    let mut tiles = Vec::new();
    for i in 0..30 {
        if i > 0 {
            tiles.push(separator().into_widget());
        }
        tiles.push(list_tile(format!("Message {i}")).subtitle("Preview of the message body").into_widget());
    }
    let detail = container()
        .color(Color::WHITE)
        .radius(BorderRadius::all(12.0))
        .shadow(shadow())
        .padding(EdgeInsets::all(20.0))
        .child(column(vec![
            text("Detail").size(24.0).bold().into_widget(),
            text("The detail pane shows the selected message with a few paragraphs of body text so the screen has realistic text density.").into_widget(),
            text("A second paragraph wraps across several lines inside the card to exercise line breaking and glyph caching.").into_widget(),
        ]).spacing(12.0));
    h.mount_widget(column(vec![
        container().padding(EdgeInsets::all(8.0)).child(toolbar).into_widget(),
        expanded(row(vec![
            container().width(260.0).child(sidebar).into_widget(),
            expanded(list_view(tiles)).into_widget(),
            container().width(420.0).padding(EdgeInsets::all(16.0)).child(detail).into_widget(),
        ]))
        .into_widget(),
    ]));
    h.settle();
}

fn landing_like(h: &mut Harness) {
    let hero = container()
        .height(260.0)
        .decoration(pebbles_render::BoxDecoration::new().gradient(Gradient::linear(
            Alignment::TOP_LEFT,
            Alignment::BOTTOM_RIGHT,
            [Color::from_rgb8(99, 102, 241), Color::from_rgb8(236, 72, 153)],
        )))
        .padding(EdgeInsets::all(40.0))
        .child(
            column(vec![
                text("Build beautiful apps").size(48.0).bold().color(Color::WHITE).into_widget(),
                button("Get started").into_widget(),
            ])
            .spacing(20.0),
        );
    let tiles = (0..6).map(|i| {
        column(vec![
            picture(procedural(160, 100, i), 160.0, 100.0).radius(BorderRadius::all(12.0)).into_widget(),
            text(format!("Category {i}")).into_widget(),
        ])
        .spacing(6.0)
        .into_widget()
    });
    let products = (0..8).map(|i| {
        container()
            .width(260.0)
            .color(Color::WHITE)
            .radius(BorderRadius::all(12.0))
            .shadow(shadow())
            .padding(EdgeInsets::all(12.0))
            .child(
                column(vec![
                    picture(procedural(236, 120, 10 + i), 236.0, 120.0).into_widget(),
                    text(format!("${}.99", 10 + i * 3)).bold().into_widget(),
                    chip("New").into_widget(),
                ])
                .spacing(8.0),
            )
            .into_widget()
    });
    let newsletter = container()
        .decoration(pebbles_render::BoxDecoration::new().gradient(Gradient::linear(
            Alignment::CENTER_LEFT,
            Alignment::CENTER_RIGHT,
            [Color::from_rgb8(16, 185, 129), Color::from_rgb8(59, 130, 246)],
        )))
        .padding(EdgeInsets::all(24.0))
        .child(
            row(vec![
                expanded(text_field().placeholder("you@example.com")).into_widget(),
                button("Subscribe").into_widget(),
            ])
            .spacing(12.0),
        );
    h.mount_widget(column(vec![
        hero.into_widget(),
        container()
            .padding(EdgeInsets::all(24.0))
            .child(wrap(tiles.collect::<Vec<_>>()).spacing(16.0))
            .into_widget(),
        container()
            .padding(EdgeInsets::all(24.0))
            .child(wrap(products.collect::<Vec<_>>()).spacing(16.0))
            .into_widget(),
        newsletter.into_widget(),
    ]));
    h.settle();
}

fn charts_like(h: &mut Harness) {
    let chart = canvas(|c| {
        let (w, hh) = (860.0, 480.0);
        let ink = Color::from_rgb8(55, 65, 81);
        let grid = Color::from_rgb8(209, 213, 219);
        c.stroke_line(Offset::new(40.0, 0.0), Offset::new(40.0, hh - 40.0), 1.0, ink);
        c.stroke_line(Offset::new(40.0, hh - 40.0), Offset::new(w, hh - 40.0), 1.0, ink);
        for k in 1..6 {
            let y = (hh - 40.0) * f64::from(k) / 6.0;
            c.stroke_line_dashed(Offset::new(40.0, y), Offset::new(w, y), 1.0, grid, &[4.0, 4.0], 0.0);
        }
        let ys = [300.0, 260.0, 280.0, 200.0, 220.0, 150.0, 170.0, 120.0];
        let mut line = BezPath::new();
        let mut area = BezPath::new();
        area.move_to((60.0, hh - 40.0));
        for (i, y) in ys.iter().enumerate() {
            let x = 60.0 + 100.0 * i as f64;
            if i == 0 {
                line.move_to((x, *y))
            } else {
                line.line_to((x, *y))
            }
            area.line_to((x, *y));
        }
        area.line_to((760.0, hh - 40.0));
        area.close_path();
        c.fill_path_gradient(
            &area,
            &Gradient::linear(
                Alignment::TOP_CENTER,
                Alignment::BOTTOM_CENTER,
                [Color::from_rgba8(59, 130, 246, 120), Color::from_rgba8(59, 130, 246, 0)],
            ),
        );
        c.stroke_path(&line, 2.0, Color::from_rgb8(37, 99, 235));
        for i in 0..8 {
            let x = 70.0 + 100.0 * f64::from(i);
            let top = 360.0 - 20.0 * f64::from(i % 4);
            c.fill_rrect(Rect::new(x, top, x + 30.0, hh - 40.0), 4.0, Color::from_rgb8(16, 185, 129));
        }
        // Pie: 6 slices as arc BezPaths.
        let (cx, cy, r) = (760.0, 110.0, 70.0);
        let colors =
            [(239, 68, 68), (245, 158, 11), (16, 185, 129), (59, 130, 246), (139, 92, 246), (236, 72, 153)];
        let mut a0 = 0.0;
        for (k, (rr, gg, bb)) in colors.iter().enumerate() {
            let a1 = a0 + std::f64::consts::TAU * [0.25, 0.2, 0.15, 0.15, 0.15, 0.1][k];
            let mut slice = BezPath::new();
            slice.move_to((cx, cy));
            kurbo::Arc::new((cx, cy), (r, r), a0, a1 - a0, 0.0).to_cubic_beziers(0.1, |p1, p2, p3| {
                if slice.elements().len() == 1 {
                    slice.line_to(Point::new(cx + r * a0.cos(), cy + r * a0.sin()));
                }
                slice.curve_to(p1, p2, p3);
            });
            slice.close_path();
            c.fill_path(&slice, Color::from_rgb8(*rr, *gg, *bb));
            a0 = a1;
        }
    })
    .width(860.0)
    .height(480.0);
    let legend = row(["Revenue", "Orders", "Visitors"].map(|l| text(l).into_widget()).to_vec()).spacing(24.0);
    h.mount_widget(
        container()
            .padding(EdgeInsets::all(20.0))
            .child(column(vec![chart.into_widget(), legend.into_widget()]).spacing(12.0)),
    );
    h.settle();
}

fn code_editor_like(h: &mut Harness) {
    let kw = Color::from_rgb8(0xC6, 0x78, 0xDD);
    let st = Color::from_rgb8(0x98, 0xC3, 0x79);
    let cm = Color::from_rgb8(0x7F, 0x84, 0x8E);
    let plain = Color::from_rgb8(0xAB, 0xB2, 0xBF);
    let mono = "JetBrains Mono";
    let mut lines = Vec::new();
    for i in 0..40 {
        let spans = match i % 4 {
            0 => vec![span("// comment line ").color(cm), span(format!("{i}")).color(cm)],
            1 => vec![span("fn ").color(kw), span(format!("item_{i}() {{")).color(plain)],
            2 => vec![span("    let s = ").color(plain), span("\"text\"").color(st), span(";").color(plain)],
            _ => vec![span("}").color(plain)],
        };
        let spans = spans.into_iter().map(|s| s.font_family(mono)).collect();
        let gutter = container()
            .width(40.0)
            .alignment(Alignment::CENTER_RIGHT)
            .child(text(format!("{}", i + 1)).size(13.0).color(cm).font_family(mono));
        let mut line = container()
            .height(18.0)
            .child(row(vec![gutter.into_widget(), text_rich(spans).size(13.0).into_widget()]).spacing(12.0));
        if i == 7 {
            line = line.color(Color::from_rgb8(44, 49, 60)); // current line
        } else if (12..15).contains(&i) {
            line = line.color(Color::from_rgba8(62, 68, 81, 200)); // selection
        }
        lines.push(line.into_widget());
    }
    let caret = container().width(2.0).height(18.0).color(Color::from_rgb8(82, 139, 255));
    h.mount_widget(
        container()
            .color(Color::from_rgb8(40, 44, 52))
            .child(column(vec![column(lines).into_widget(), caret.into_widget()])),
    );
    h.settle();
}

fn markdown_like(h: &mut Harness) {
    let code_bg = Color::from_rgb8(243, 244, 246);
    h.mount_widget(
        container().padding(EdgeInsets::all(32.0)).child(
            column(vec![
                text("Heading one").size(32.0).bold().into_widget(),
                text("Heading two").size(24.0).bold().into_widget(),
                text("Heading three").size(18.0).bold().into_widget(),
                text_rich(vec![
                    span("A paragraph with "),
                    span("inline_code").font_family("JetBrains Mono"),
                    span(" and a "),
                    span("link").underline().color(Color::from_rgb8(37, 99, 235)),
                    span("."),
                ])
                .into_widget(),
                text("• bullet one").into_widget(),
                text("• bullet two").into_widget(),
                text("1. numbered").into_widget(),
                text("2. numbered").into_widget(),
                row(vec![
                    container().width(4.0).height(40.0).color(Color::from_rgb8(209, 213, 219)).into_widget(),
                    text("A blockquote with a left border.").into_widget(),
                ])
                .spacing(12.0)
                .into_widget(),
                container()
                    .color(code_bg)
                    .radius(BorderRadius::all(8.0))
                    .padding(EdgeInsets::all(12.0))
                    .child(text("fn main() {\n    println!(\"hi\");\n}").font_family("JetBrains Mono"))
                    .into_widget(),
                table(vec!["Col A".into(), "Col B".into()]).row(["1", "2"]).row(["3", "4"]).into_widget(),
                separator().into_widget(),
            ])
            .spacing(12.0),
        ),
    );
    h.settle();
}

fn form(h: &mut Harness) {
    h.mount_widget(
        container().padding(EdgeInsets::all(32.0)).child(
            column(vec![
                text_field().placeholder("Name").into_widget(),
                text_field().value("ada@example.com").into_widget(),
                text_field().value("not-an-email").into_widget(),
                text_field().placeholder("Message").into_widget(),
                row(vec![checkbox(true).into_widget(), text("Subscribe").into_widget()])
                    .spacing(8.0)
                    .into_widget(),
                row(vec![
                    radio(true).into_widget(),
                    text("Monthly").into_widget(),
                    radio(false).into_widget(),
                    text("Yearly").into_widget(),
                ])
                .spacing(8.0)
                .into_widget(),
                row(vec![switch(true).into_widget(), text("Notifications").into_widget()])
                    .spacing(8.0)
                    .into_widget(),
                slider(240.0).into_widget(),
                row(vec![
                    button("Submit").into_widget(),
                    button("Cancel").variant(pebbles_widgets::ButtonVariant::Secondary).into_widget(),
                ])
                .spacing(12.0)
                .into_widget(),
            ])
            .spacing(16.0),
        ),
    );
    h.press_tab();
    h.settle();
}

// ---- Flutter-parity screens (V53 step 3, DR-24) ----------------------------------------------

/// A Flutter `Scaffold(extendBodyBehindAppBar)` with a frosted `BackdropFilter` app bar over a
/// scrolling photo list: the blur samples the photos under the bar.
fn frosted_app_bar(h: &mut Harness) {
    // The list starts at the top edge, so the first photos sit under the translucent bar.
    let mut items = Vec::new();
    for i in 0..8u32 {
        items.push(
            container()
                .margin(EdgeInsets::symmetric(16.0, 8.0))
                .color(Color::WHITE)
                .radius(BorderRadius::all(16.0))
                .shadow(shadow())
                .child(row(vec![
                    picture(procedural(160, 110, i + 3), 160.0, 110.0).into_widget(),
                    container()
                        .padding(EdgeInsets::all(16.0))
                        .child(
                            column(vec![
                                text(format!("Photo {}", i + 1)).size(18.0).bold().into_widget(),
                                text("Golden hour over the harbour, shot on a long lens.").into_widget(),
                            ])
                            .spacing(6.0),
                        )
                        .into_widget(),
                ]))
                .into_widget(),
        );
    }
    let bar = backdrop_filter(
        pebbles_render::LayerEffect::Blur {
            sigma_x: 10.0,
            sigma_y: 10.0,
            edge: pebbles_render::BlurEdge::Clamp,
        },
        container()
            .height(64.0)
            .color(Color::from_rgba8(255, 255, 255, 140))
            .padding(EdgeInsets::symmetric(16.0, 0.0))
            .child(row(vec![
                icon_button(tabler::MENU_2).into_widget(),
                expanded(text("Gallery").size(20.0).bold()).into_widget(),
                icon_button(tabler::SEARCH).into_widget(),
                icon_button(tabler::DOTS_VERTICAL).into_widget(),
            ])),
    );
    h.mount_widget(stack(vec![
        list_view(items).into_widget(),
        positioned(bar).left(0.0).top(0.0).right(0.0).into_widget(),
    ]));
}

/// Flutter's `Matrix4.identity()..setEntry(3, 2, 0.001)..rotateY(a)` (column-major).
fn perspective_rotate_y(a: f64) -> [f64; 16] {
    let (s, c) = a.sin_cos();
    // P · Ry, P = identity with row 3 / column 2 = 0.001.
    let p = 0.001;
    [c, 0.0, -s, -p * s, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, p * c, 0.0, 0.0, 0.0, 1.0]
}

fn stack_card(title: &str, seed: u32) -> pebbles_widgets::Container {
    container()
        .width(220.0)
        .height(300.0)
        .color(Color::WHITE)
        .radius(BorderRadius::all(18.0))
        .shadow(BoxShadow::new(Color::from_rgba8(0, 0, 0, 60), Offset::new(0.0, 10.0), 24.0, 0.0))
        .child(column(vec![
            picture(procedural(220, 180, seed), 220.0, 180.0).into_widget(),
            container()
                .padding(EdgeInsets::all(14.0))
                .child(
                    column(vec![
                        text(title).size(18.0).bold().into_widget(),
                        text("Tap to flip").into_widget(),
                    ])
                    .spacing(4.0),
                )
                .into_widget(),
        ]))
}

/// A card stack: shadowed cards, the middle one mid-flip in perspective (Flutter's flip-card
/// pattern), one rotated in plane and overlapping its neighbour.
fn card_stack(h: &mut Harness) {
    h.mount_widget(container().color(Color::from_rgb8(238, 241, 246)).padding(EdgeInsets::all(40.0)).child(
        stack(vec![
            positioned(stack_card("Lisbon", 1)).left(20.0).top(60.0).into_widget(),
            positioned(Transform::matrix4(perspective_rotate_y(0.6), stack_card("Kyoto", 2)))
                .left(300.0)
                .top(40.0)
                .into_widget(),
            positioned(Transform::rotate(-0.12, stack_card("Oslo", 3))).left(560.0).top(70.0).into_widget(),
        ]),
    ));
}

/// A text-heavy article: decorations of every style, text shadows, highlight and inline icons.
fn article(h: &mut Harness) {
    use pebbles_render::{TextDecorationStyle as D, TextShadow};
    let shade = |dy: f64, blur: f32, a: u8| TextShadow {
        offset: Offset::new(0.0, dy),
        blur_sigma: blur,
        color: Color::from_rgba8(0, 0, 0, a),
    };
    let accent = Color::from_rgb8(220, 38, 38);
    let para = |spans| text_rich(spans).into_widget();
    h.mount_widget(
        container().color(Color::WHITE).padding(EdgeInsets::all(40.0)).child(
            column(vec![
                text_rich(vec![span("The Quiet Harbour").size(40.0).bold().shadow(shade(3.0, 4.0, 70))])
                    .into_widget(),
                para(vec![
                    widget_span(|| icon(tabler::CALENDAR).into_widget()),
                    span(" 12 March  "),
                    widget_span(|| icon(tabler::CLOCK).into_widget()),
                    span(" 6 min read").color(Color::from_rgb8(100, 116, 139)),
                ]),
                para(vec![
                    span("Morning fog lifts slowly. The "),
                    span("ferries").underline(),
                    span(" leave on time, the "),
                    span("fishermen").underline().decoration_style(D::Wavy).decoration_color(accent),
                    span(" mend their nets, and the "),
                    span("market").underline().decoration_style(D::Dotted),
                    span(" fills before nine. A "),
                    span("double-underlined").underline().decoration_style(D::Double),
                    span(" warning about the "),
                    span("tide").underline().decoration_style(D::Dashed).decoration_thickness(2.0),
                    span(" hangs by the pier."),
                ]),
                para(vec![
                    span("Prices were "),
                    span("€4.20").strikethrough().decoration_color(accent),
                    span(" now €3.50 — an "),
                    span("overlined").overline(),
                    span(" note, a "),
                    span("highlighted phrase").background(Color::from_rgb8(254, 240, 138)),
                    span(" and a "),
                    span("glowing").color(Color::WHITE).shadow(shade(0.0, 3.0, 200)),
                    span(" word close the paragraph."),
                ]),
                container()
                    .padding(EdgeInsets::all(20.0))
                    .color(Color::from_rgb8(248, 250, 252))
                    .radius(BorderRadius::all(12.0))
                    .child(text_rich(vec![
                        widget_span(|| icon(tabler::QUOTE).into_widget()),
                        span(" The sea does not hurry, yet everything arrives.").italic().size(20.0),
                    ]))
                    .into_widget(),
                para(vec![
                    span("Tabular figures "),
                    span("0123456789").font_feature(*b"tnum", 1),
                    span(" and small caps "),
                    span("Harbour Office").font_feature(*b"smcp", 1),
                    span(" round out the typography."),
                ]),
            ])
            .spacing(18.0)
            .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start),
        ),
    );
}

#[cfg(test)]
mod tests {
    /// No widget/screen case may touch the network (V25 step 4).
    #[test]
    fn screen_cases_are_offline() {
        for src in [include_str!("widget.rs"), include_str!("screen.rs")] {
            let banned = ["http:", "https:", "ImageView::network", "network("];
            for b in banned {
                let hits = src.lines().filter(|l| l.contains(b) && !l.contains("let banned")).count();
                assert_eq!(hits, 0, "{b} found in a corpus source");
            }
        }
    }
}
