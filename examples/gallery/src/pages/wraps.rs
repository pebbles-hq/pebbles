use pebbles::prelude::*;

use crate::ui::{doc, gap_h, screen};

pub fn wraps() -> Element {
    screen("Wrap").body(children![chips(), tag_cloud(), reflow(), avatars(),])
}

fn tags() -> Vec<Badge> {
    ["design", "rust", "wgpu", "gpu", "widgets", "mobile", "desktop", "theming", "layout", "reactive"]
        .into_iter()
        .map(|t| badge(t).variant(BadgeVariant::Secondary))
        .collect()
}

fn chips() -> impl IntoWidget {
    doc("Chips").body(wrap(tags()).spacing(8.0).run_spacing(8.0))
}

fn tag_cloud() -> impl IntoWidget {
    let th = theme();
    doc("Mixed-size children").body(
        wrap(vec![
            badge("rust").into_widget(),
            badge("widgets & layout").variant(BadgeVariant::Secondary).into_widget(),
            button("reactive").size(ButtonSize::Sm).into_widget(),
            badge("wgpu gpu renderer").variant(BadgeVariant::Secondary).into_widget(),
            container()
                .padding(EdgeInsets::symmetric(4.0, 10.0))
                .decoration(BoxDecoration::new().color(th.colors.secondary).radius(BorderRadius::all(999.0)))
                .child(text("composable").size(12.5).weight(500.0))
                .into_widget(),
            badge("theming").variant(BadgeVariant::Secondary).into_widget(),
            badge("a much longer tag that takes a whole line").variant(BadgeVariant::Secondary).into_widget(),
            badge("desktop").variant(BadgeVariant::Secondary).into_widget(),
            badge("parity").variant(BadgeVariant::Secondary).into_widget(),
        ])
        .spacing(8.0)
        .run_spacing(8.0),
    )
}

fn reflow() -> impl IntoWidget {
    doc("Responsive reflow").body(
        row(children![
            column(children![
                container()
                    .width(180.0)
                    .padding(EdgeInsets::all(6.0))
                    .decoration(
                        BoxDecoration::new()
                            .border(Border::new(theme().colors.border, 1.0))
                            .radius(BorderRadius::all(theme().radius)),
                    )
                    .child(wrap(tags()).spacing(6.0).run_spacing(6.0))
                    .into_widget(),
                gap_h(6.0),
                muted("width 180").size(11.0).into_widget(),
            ])
            .main_axis_size(MainAxisSize::Min),
            container().width(24.0).into_widget(),
            column(children![
                container()
                    .width(420.0)
                    .padding(EdgeInsets::all(6.0))
                    .decoration(
                        BoxDecoration::new()
                            .border(Border::new(theme().colors.border, 1.0))
                            .radius(BorderRadius::all(theme().radius)),
                    )
                    .child(wrap(tags()).spacing(6.0).run_spacing(6.0))
                    .into_widget(),
                gap_h(6.0),
                muted("width 420").size(11.0).into_widget(),
            ])
            .main_axis_size(MainAxisSize::Min),
        ])
        .main_axis_size(MainAxisSize::Min),
    )
}

fn avatars() -> impl IntoWidget {
    let th = theme();
    let names = ["RK", "Ana", "Bo", "Li", "Zoe", "Kim", "Max", "Sam", "Ivy", "Noe", "Gus", "Uma"];
    doc("Avatar rail").body(
        wrap(
            names
                .into_iter()
                .enumerate()
                .map(|(i, n)| {
                    let colors = [
                        palette::BLUE,
                        palette::GREEN,
                        palette::AMBER,
                        palette::PURPLE,
                        palette::TEAL,
                        palette::INDIGO,
                    ];
                    container()
                        .width(36.0)
                        .height(36.0)
                        .decoration(
                            BoxDecoration::new()
                                .color(colors[i % colors.len()])
                                .shape(BoxShape::Circle)
                                .border(Border::new(th.colors.card, 2.0)),
                        )
                        .child(center(text(n.to_string()).color(palette::WHITE).size(11.0).semibold()))
                        .into_widget()
                })
                .collect::<Vec<_>>(),
        )
        .spacing(6.0)
        .run_spacing(6.0),
    )
}
