use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn tabs_screen() -> Element {
    let tab = create_signal(0usize);
    let pill = create_signal(0usize);

    screen("Tabs")
        .description("A tab bar plus the selected tab's content — controlled (selection in, on_select out), keyboard-navigable (focus the strip, Left/Right switch), with a cross-fading content area.")
        .body(children![
            doc("Underline — the default")
                .description("The classic shadcn look: the active tab is underlined in the primary color. Focus the strip (Tab) then use Left/Right — disabled tabs are skipped.")
                .body(
                    column(children![
                        tabs(tab.get())
                            .tab(
                                "Account",
                                body("Make changes to your account here."),
                                move || tab.set(0)
                            )
                            .tab("Password", body("Change your password here."), move || tab
                                .set(1))
                            .tab("Team", body("Manage your team members."), move || tab
                                .set(2)),
                        gap_h(8.0),
                        muted(format!("selected tab: {}", tab.get())),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("Pills")
                .description(".variant(TabsVariant::Pills) — the active tab sits in a rounded pill instead of an underline.")
                .body(
                    tabs(pill.get())
                        .variant(TabsVariant::Pills)
                        .tab("Music", body("Your saved library."), move || pill.set(0))
                        .tab("Podcasts", body("Shows you follow."), move || pill.set(1))
                        .tab("Audiobooks", body("Unavailable on this plan."), move || {
                            pill.set(2)
                        })
                        .tab_disabled(2),
                ),
            doc("Styled strip")
                .description("A Style covers the strip — background, border, radius — and its text props style the labels (color, size, weight).")
                .body(
                    column(children![
                        tabs(tab.get())
                            .tab(
                                "Account",
                                body("Make changes to your account here."),
                                move || tab.set(0)
                            )
                            .tab("Password", body("Change your password here."), move || tab
                                .set(1))
                            .tab("Team", body("Manage your team members."), move || tab
                                .set(2))
                            .style(
                                style()
                                    .background(theme().colors.card)
                                    .border(Border::new(theme().colors.border, 1.0))
                                    .radius_all(theme().radius)
                                    .color(palette::blue::S600)
                                    .font_weight(600.0),
                            ),
                        gap_h(16.0),
                        tabs(pill.get())
                            .variant(TabsVariant::Pills)
                            .tab("Music", body("Your library."), move || pill.set(0))
                            .tab("Podcasts", body("Your shows."), move || pill.set(1))
                            .style(
                                style()
                                    .background(palette::violet::S600)
                                    .radius_all(999.0)
                                    .padding_xy(4.0, 4.0)
                                    .color(palette::WHITE),
                            ),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
        ])
}
