//! A **desktop-structured** app at desktop size (1240×820) — the classic app shell:
//! a left `SideNav` (collapsible rail), a top bar, and a content area. This is the
//! shape a real desktop app (dashboard, admin, IDE) takes.

use pebbles::prelude::*;

fn app() -> impl IntoWidget {
    let section = create_signal(0_usize);
    let collapsed = create_signal(false);
    let titles = ["Dashboard", "Analytics", "Customers", "Settings"];
    let c = theme().colors;

    scaffold(content(section.get()))
        .side(
            side_nav()
                .width(230.0)
                .collapsible(true)
                .collapsed(collapsed.get())
                .on_collapse_changed(move |v| collapsed.set(v))
                .header(brand())
                .item(nav(lucide::LAYOUT_DASHBOARD, "Dashboard", 0, section))
                .item(nav(lucide::TRENDING_UP, "Analytics", 1, section))
                .item(nav(lucide::USERS, "Customers", 2, section))
                .item(nav(lucide::SETTINGS, "Settings", 3, section))
                .footer(
                    row(children![
                        avatar("RS").color(palette::violet::S500),
                        gap_w(10.0),
                        column(children![
                            text("Reyco").size(13.5).semibold().color(c.foreground),
                            text("Pro plan").size(12.0).color(c.muted_foreground),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .main_axis_size(MainAxisSize::Min),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Center),
                ),
        )
        .top(
            top_panel(titles[section.get().min(3)])
                .action(icon_button(lucide::SEARCH).variant(ButtonVariant::Ghost))
                .action(icon_button(lucide::BELL).variant(ButtonVariant::Ghost))
                .action(button("New").leading(lucide::PLUS)),
        )
        .background(c.background)
}

fn brand() -> impl IntoWidget {
    row(children![
        container()
            .decoration(BoxDecoration::new().color(theme().colors.primary).radius(BorderRadius::all(8.0)))
            .padding(EdgeInsets::all(6.0))
            .child(icon(lucide::LAYERS).size(18.0).color(theme().colors.primary_foreground)),
        gap_w(10.0),
        text("Pebbles").size(16.0).weight(700.0).color(theme().colors.foreground),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
}

fn nav(icon: IconData, label: &str, index: usize, section: Signal<usize>) -> NavItem {
    nav_item(label).icon(icon).selected(section.get() == index).on_select(move || section.set(index))
}

fn content(section: usize) -> AnyWidget {
    match section {
        0 => dashboard().into_widget(),
        _ => placeholder(["Dashboard", "Analytics", "Customers", "Settings"][section.min(3)]).into_widget(),
    }
}

// ---------------------------------------------------------------------------
// Dashboard — a row of stat cards + a recent-activity card
// ---------------------------------------------------------------------------

fn dashboard() -> impl IntoWidget {
    let stats = [
        ("Revenue", "$48.2k", "+12.5%", lucide::TRENDING_UP, palette::emerald::S500),
        ("Users", "2,340", "+4.1%", lucide::USERS, palette::sky::S500),
        ("Orders", "1,204", "-2.3%", lucide::INBOX, palette::amber::S500),
        ("Uptime", "99.98%", "stable", lucide::ACTIVITY, palette::violet::S500),
    ];
    let cards: Vec<AnyWidget> =
        stats.iter().map(|&s| stat_card(s.0, s.1, s.2, s.3, s.4).into_widget()).collect();

    let activity = card().title("Recent activity").description("The last few things that happened").child(
        column(
            [
                ("Deployed v0.2.0 to production", "2m ago", palette::emerald::S500),
                ("New customer: Acme Corp", "18m ago", palette::sky::S500),
                ("Invoice #1042 paid", "1h ago", palette::violet::S500),
                ("Nightly backup completed", "3h ago", palette::zinc::S400),
            ]
            .iter()
            .map(|&(what, when, color)| activity_row(what, when, color).into_widget())
            .collect::<Vec<_>>(),
        )
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min),
    );

    scroll_view(
        container().padding(EdgeInsets::all(24.0)).child(
            column(children![wrap(cards).spacing(16.0).run_spacing(16.0), gap_h(20.0), activity,])
                .cross_axis_alignment(CrossAxisAlignment::Stretch)
                .main_axis_size(MainAxisSize::Min),
        ),
    )
}

fn stat_card(label: &str, value: &str, delta: &str, ic: IconData, color: Color) -> impl IntoWidget {
    let c = theme().colors;
    let up = !delta.starts_with('-');
    container()
        .decoration(
            BoxDecoration::new()
                .color(c.card)
                .border(Border::new(c.border, 1.0))
                .radius(BorderRadius::all(14.0)),
        )
        .padding(EdgeInsets::all(18.0))
        .width(230.0)
        .child(
            column(children![
                row(children![
                    text(label.to_string()).size(13.0).color(c.muted_foreground),
                    spacer(),
                    icon(ic).size(18.0).color(color),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Center),
                gap_h(10.0),
                text(value.to_string()).size(26.0).weight(700.0).color(c.foreground),
                gap_h(4.0),
                text(delta.to_string()).size(12.5).color(if up {
                    palette::emerald::S600
                } else {
                    palette::rose::S500
                }),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        )
}

fn activity_row(what: &str, when: &str, color: Color) -> impl IntoWidget {
    let c = theme().colors;
    container().padding(EdgeInsets::symmetric(0.0, 10.0)).child(
        row(children![
            container()
                .decoration(BoxDecoration::new().color(color).shape(BoxShape::Circle))
                .width(8.0)
                .height(8.0),
            gap_w(12.0),
            text(what.to_string()).size(14.0).color(c.foreground),
            spacer(),
            text(when.to_string()).size(12.5).color(c.muted_foreground),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center),
    )
}

fn placeholder(title: &str) -> impl IntoWidget {
    let c = theme().colors;
    center(
        column(children![
            icon(lucide::LAYERS).size(40.0).color(c.muted_foreground),
            gap_h(12.0),
            text(title.to_string()).size(20.0).semibold().color(c.foreground),
            text("This section is a placeholder in the sample.").size(13.5).color(c.muted_foreground),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_size(MainAxisSize::Min),
    )
}

#[pebbles::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(component(app)).title("Pebbles — Desktop").size(1240, 820).run()
}
