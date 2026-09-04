use pebbles::prelude::*;

use crate::ui::{doc, gap_h, screen};

pub fn scrolling() -> Element {
    screen("Scroll & Motion")
        .description(
            "The scrolling depth tier: drag-to-scroll with fling + rubber-band overscroll (A4), pull-to-refresh (A5), the paged Carousel (A6), and sticky + collapsing headers (A3).",
        )
        .body(children![
            drag_scroll(),
            pull_to_refresh(),
            carousel_demo(),
            sticky_headers(),
            collapsing_demo(),
        ])
}

fn drag_scroll() -> impl IntoWidget {
    doc("Drag & fling")
        .description(".drag_scroll(true) turns the viewport into a pan-to-scroll surface: drag the rows 1:1, fling to glide, and (with .physics(overscroll)) pull past the edges — they rubber-band and spring back. Wheel input stays hard-clamped.")
        .body(
            container()
                .height(200.0)
                .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                .child(
                    scroll_view(
                        column((0..40).map(|i| {
                            padding(
                                EdgeInsets::symmetric(12.0, 8.0),
                                muted(format!("row {i} — drag me")).into_widget(),
                            )
                            .into_widget()
                        }).collect::<Vec<_>>())
                        .main_axis_size(MainAxisSize::Min),
                    )
                    .drag_scroll(true)
                    .physics(ScrollPhysics { overscroll: true, friction: 0.01, ..Default::default() }),
                ),
        )
}

fn pull_to_refresh() -> impl IntoWidget {
    let refreshing = create_signal(false);
    doc("Pull-to-refresh")
        .description("refresh_indicator(child).on_refresh(..) — drag the list down past 64px (the threshold) and release: the spinner row holds until the app finishes the refresh. Try it with the mouse: press, pull down, let go.")
        .body(
            container()
                .height(200.0)
                .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                .child(refresh_indicator(
                    column((0..30).map(|i| {
                        padding(
                            EdgeInsets::symmetric(12.0, 8.0),
                            muted(format!("item {i}")).into_widget(),
                        )
                        .into_widget()
                    }).collect::<Vec<_>>())
                    .main_axis_size(MainAxisSize::Min),
                )
                .threshold(64.0)
                .on_refresh(move |done| {
                    refreshing.set(true);
                    let done = done;
                    let refreshing = refreshing;
                    spawn(
                        move || std::thread::sleep(std::time::Duration::from_millis(1200)),
                        move |_| {
                            refreshing.set(false);
                            done.finish();
                        },
                    );
                })),
        )
}

fn carousel_demo() -> impl IntoWidget {
    let page = create_signal(0usize);
    doc("Carousel")
        .description("carousel(children![]) — snap-paged slides with dots, prev/next arrows (hidden at the ends), and optional .autoplay(..) that pauses while hovered.")
        .body(column(children![
            carousel(children![
                slide("Signals", "state without setState", palette::BLUE),
                slide("Widgets", "Flutter's vocabulary, Rust-shaped", palette::GREEN),
                slide("Vello", "the GPU renderer underneath", palette::AMBER),
                slide("Gallery", "~50 screens of the catalog", palette::PURPLE),
            ])
            .height(150.0)
            .autoplay(3.5)
            .on_page_changed(move |p| page.set(p)),
            gap_h(8.0),
            muted(format!("page {} of 4", page.get() + 1)),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min))
}

fn slide(title: &'static str, sub: &'static str, color: Color) -> impl IntoWidget {
    center(column(children![
        text(title).size(20.0).bold().color(palette::WHITE),
        gap_h(4.0),
        muted(sub).size(12.0).color(Color::from_rgba8(255, 255, 255, 200)),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .main_axis_size(MainAxisSize::Min))
    .styled(style().background(color))
}

fn sticky_headers() -> impl IntoWidget {
    doc("Sticky headers")
        .description("sticky_list() — each section's header pins to the top while its rows are visible, then slides away as the next header pushes it off. v1: headers share one extent, rows share one.")
        .body(
            container()
                .height(220.0)
                .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                .child(
                    sticky_list()
                        .section(section_header("GETTING STARTED"), rows(&["Introduction", "Installation", "The prelude"]))
                        .section(section_header("WIDGETS"), rows(&["Container", "Row & Column", "Stack", "ListView"]))
                        .section(section_header("STYLING"), rows(&["Theme", "Style", "Palette", "Gradients"]))
                        .section(section_header("SERVICES"), rows(&["Dialogs", "Toasts", "Sheets"]))
                        .header_extent(36.0)
                        .row_extent(40.0),
                ),
        )
}

fn rows(labels: &[&str]) -> Vec<AnyWidget> {
    labels
        .iter()
        .map(|l| {
            padding(
                EdgeInsets::symmetric(16.0, 10.0),
                muted(*l).into_widget(),
            )
            .into_widget()
        })
        .collect()
}

fn collapsing_demo() -> impl IntoWidget {
    doc("Collapsing header")
        .description("collapsing_header(expanded, collapsed, |t| hero(t)) — the pinned hero shrinks from 240px to 64px as you scroll; the builder receives the progress t (0..1) and can fade/scale its contents.")
        .body(
            container()
                .height(220.0)
                .decoration(BoxDecoration::new().border(Border::new(theme().colors.border, 1.0)).radius(BorderRadius::all(theme().radius)))
                .child(
                    collapsing_header(180.0, 56.0, move |t| {
                        let c = theme().colors;
                        center(column(children![
                            text("A collapsing profile").size(18.0).bold().color(palette::WHITE),
                            gap_h(4.0),
                            opacity(t as f32, muted(format!("scroll progress {t:.2}")).size(11.0).color(Color::from_rgba8(255, 255, 255, 220))),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .main_axis_size(MainAxisSize::Min))
                        .styled(style().background(c.primary))
                    })
                    .content((0..20).map(|i| {
                        padding(
                            EdgeInsets::symmetric(16.0, 10.0),
                            muted(format!("profile row {i}")).into_widget(),
                        )
                        .into_widget()
                    }).collect::<Vec<_>>()),
                ),
        )
}
