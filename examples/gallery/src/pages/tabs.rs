use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn tabs_screen() -> Element {
    let tab = create_signal(0usize);
    let pill = create_signal(0usize);
    let solid = create_signal(0usize);

    screen("Tabs").body(children![
        doc("Underline").body(
            column(children![
                tabs(tab.get())
                    .tab("Account", body("Make changes to your account here."), move || tab.set(0))
                    .tab("Password", body("Change your password here."), move || tab.set(1))
                    .tab("Team", body("Manage your team members."), move || tab.set(2)),
                gap_h(8.0),
                muted(format!("selected tab: {}", tab.get())),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min),
        ),
        doc("Pills").body(
            tabs(pill.get())
                .variant(TabsVariant::Pills)
                .tab("Music", body("Your saved library."), move || pill.set(0))
                .tab("Podcasts", body("Shows you follow."), move || pill.set(1))
                .tab("Audiobooks", body("Unavailable on this plan."), move || { pill.set(2) })
                .tab_disabled(2),
        ),
        doc("Solid").body(
            tabs(solid.get())
                .variant(TabsVariant::Solid)
                .tab("Chat", body("The conversation."), move || solid.set(0))
                .tab("Files", body("Shared documents."), move || solid.set(1))
                .tab("Members", body("The team roster."), move || solid.set(2)),
        ),
        doc("active_color").body(
            column(children![
                tabs(tab.get())
                    .tab("Account", body("Make changes to your account here."), move || tab.set(0))
                    .tab("Password", body("Change your password here."), move || tab.set(1))
                    .active_color(palette::emerald::S600),
                gap_h(16.0),
                tabs(pill.get())
                    .variant(TabsVariant::Pills)
                    .tab("Music", body("Your library."), move || pill.set(0))
                    .tab("Podcasts", body("Your shows."), move || pill.set(1))
                    .active_color(palette::rose::S600),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min),
        ),
        doc("Styled").body(
            column(children![
                tabs(tab.get())
                    .tab("Account", body("Make changes to your account here."), move || tab.set(0))
                    .tab("Password", body("Change your password here."), move || tab.set(1))
                    .style(
                        style()
                            .background(theme().colors.card)
                            .border(Border::new(theme().colors.border, 1.0))
                            .radius_all(theme().radius)
                            .color(palette::blue::S600)
                            .font_weight(600.0),
                    ),
                gap_h(16.0),
                tabs(solid.get())
                    .variant(TabsVariant::Solid)
                    .tab("Chat", body("The conversation."), move || solid.set(0))
                    .tab("Files", body("Shared documents."), move || solid.set(1))
                    .tab_padding(EdgeInsets::symmetric(20.0, 8.0))
                    .content_padding(EdgeInsets::symmetric(8.0, 16.0))
                    .active_color(palette::violet::S600)
                    .style(style().background(palette::violet::S600).color(palette::WHITE),),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min),
        ),
    ])
}
