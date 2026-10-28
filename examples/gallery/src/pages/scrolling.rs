use pebbles::prelude::*;

use crate::ui::{doc, gap_h, screen};

pub fn scrolling() -> Element {
    screen("Scroll & Motion").body(children![
        drag_scroll(),
        pull_to_refresh(),
        carousel_demo(),
        sticky_headers(),
        collapsing_demo(),
    ])
}

fn drag_scroll() -> impl IntoWidget {
    doc("Drag & fling").body(
        container()
            .height(200.0)
            .decoration(
                BoxDecoration::new()
                    .border(Border::new(theme().colors.border, 1.0))
                    .radius(BorderRadius::all(theme().radius)),
            )
            .child(
                scroll_view(
                    column(
                        (0..40)
                            .map(|i| {
                                padding(
                                    EdgeInsets::symmetric(12.0, 8.0),
                                    muted(format!("row {i} — drag me")).into_widget(),
                                )
                                .into_widget()
                            })
                            .collect::<Vec<_>>(),
                    )
                    .main_axis_size(MainAxisSize::Min),
                )
                .drag_scroll(true)
                .physics(ScrollPhysics {
                    overscroll: true,
                    friction: 0.01,
                    ..Default::default()
                }),
            ),
    )
}

fn pull_to_refresh() -> impl IntoWidget {
    let refreshing = create_signal(false);
    doc("Pull-to-refresh").body(
        container()
            .height(200.0)
            .decoration(
                BoxDecoration::new()
                    .border(Border::new(theme().colors.border, 1.0))
                    .radius(BorderRadius::all(theme().radius)),
            )
            .child(
                refresh_indicator(
                    column(
                        (0..30)
                            .map(|i| {
                                padding(
                                    EdgeInsets::symmetric(12.0, 8.0),
                                    muted(format!("item {i}")).into_widget(),
                                )
                                .into_widget()
                            })
                            .collect::<Vec<_>>(),
                    )
                    .main_axis_size(MainAxisSize::Min),
                )
                .threshold(64.0)
                .on_refresh(move |done| {
                    refreshing.set(true);
                    // Simulate a 1.2s refresh. Do NOT use std::thread::sleep: on the
                    // wasm build it lowers to `memory.atomic.wait` on non-shared memory,
                    // which TRAPS and aborts the whole app on web (every page then looks
                    // broken). A frame-loop timer behaves identically on native + web.
                    const REFRESH_TIMER: u64 = u64::from_le_bytes(*b"pullref!");
                    set_timeout(REFRESH_TIMER, 1.2, move || {
                        refreshing.set(false);
                        done.finish();
                    });
                }),
            ),
    )
}

fn carousel_demo() -> impl IntoWidget {
    let page = create_signal(0usize);
    doc("Carousel").body(
        column(children![
            carousel(children![
                slide("Signals", "state without setState", palette::BLUE),
                slide("Widgets", "a familiar widget vocabulary", palette::GREEN),
                slide("wgpu", "the GPU renderer underneath", palette::AMBER),
                slide("Gallery", "every component, one page each", palette::PURPLE),
            ])
            .height(150.0)
            .autoplay(3.5)
            .on_page_changed(move |p| page.set(p)),
            gap_h(8.0),
            muted(format!("page {} of 4", page.get() + 1)),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min),
    )
}

fn slide(title: &'static str, sub: &'static str, color: Color) -> impl IntoWidget {
    center(
        column(children![
            text(title).size(20.0).bold().color(palette::WHITE),
            gap_h(4.0),
            muted(sub).size(12.0).color(Color::from_rgba8(255, 255, 255, 200)),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_size(MainAxisSize::Min),
    )
    .styled(style().background(color))
}

fn sticky_headers() -> impl IntoWidget {
    doc("Sticky headers").body(
        container()
            .height(220.0)
            .decoration(
                BoxDecoration::new()
                    .border(Border::new(theme().colors.border, 1.0))
                    .radius(BorderRadius::all(theme().radius)),
            )
            .child(
                sticky_list()
                    .section(
                        section_header("GETTING STARTED"),
                        rows(&["Introduction", "Installation", "The prelude"]),
                    )
                    .section(
                        section_header("WIDGETS"),
                        rows(&["Container", "Row & Column", "Stack", "ListView"]),
                    )
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
        .map(|l| padding(EdgeInsets::symmetric(16.0, 10.0), muted(*l).into_widget()).into_widget())
        .collect()
}

fn collapsing_demo() -> impl IntoWidget {
    doc("Collapsing header").body(
        container()
            .height(220.0)
            .decoration(
                BoxDecoration::new()
                    .border(Border::new(theme().colors.border, 1.0))
                    .radius(BorderRadius::all(theme().radius)),
            )
            .child(
                collapsing_header(180.0, 56.0, move |t| {
                    let c = theme().colors;
                    center(
                        column(children![
                            text("A collapsing profile").size(18.0).bold().color(palette::WHITE),
                            gap_h(4.0),
                            opacity(
                                t as f32,
                                muted(format!("scroll progress {t:.2}"))
                                    .size(11.0)
                                    .color(Color::from_rgba8(255, 255, 255, 220))
                            ),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .main_axis_size(MainAxisSize::Min),
                    )
                    .styled(style().background(c.primary))
                })
                .content(
                    (0..20)
                        .map(|i| {
                            padding(
                                EdgeInsets::symmetric(16.0, 10.0),
                                muted(format!("profile row {i}")).into_widget(),
                            )
                            .into_widget()
                        })
                        .collect::<Vec<_>>(),
                ),
            ),
    )
}
