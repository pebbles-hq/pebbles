use pebbles::prelude::*;

use crate::ui::{screen, section};

pub fn navigation() -> Element {
    let tab = create_signal(0usize);
    let pill = create_signal(0usize);
    let acc = create_signal([true, false, false]);
    let pg = create_signal(2usize);

    screen(
        "Navigation",
        "Tabs, accordion, breadcrumb, pagination.",
        children![
            section(
                "TABS",
                column(children![
                    tabs(tab.get())
                        .tab("Account", body("Make changes to your account here."), move || tab.set(0))
                        .tab("Password", body("Change your password here."), move || tab.set(1))
                        .tab("Team", body("Manage your team members."), move || tab.set(2)),
                    gap_h(28.0),
                    tabs(pill.get())
                        .variant(TabsVariant::Pills)
                        .tab("Music", body("Your saved library."), move || pill.set(0))
                        .tab("Podcasts", body("Shows you follow."), move || pill.set(1))
                        .tab("Audiobooks", body("Unavailable on this plan."), move || pill.set(2))
                        .tab_disabled(2),
                ]),
            ),
            section(
                "ACCORDION",
                accordion()
                    .item("Is it accessible?", muted("Yes. It follows the box protocol."), acc.get()[0], move || acc.update(|a| a[0] = !a[0]))
                    .item("Is it styled?", muted("Yes, from theme tokens."), acc.get()[1], move || acc.update(|a| a[1] = !a[1]))
                    .item("Is it animated?", muted("Not yet — on the roadmap."), acc.get()[2], move || acc.update(|a| a[2] = !a[2])),
            ),
            section("BREADCRUMB", breadcrumb(vec!["Home".into(), "Projects".into(), "Pebbles".into()])),
            section(
                "MENUBAR",
                menubar()
                    .menu("File", [menu_item("New").shortcut("⌘N"), menu_item("Open"), menu_item("Save")])
                    .menu("Edit", [menu_item("Undo"), menu_item("Redo")])
                    .menu("View", [menu_item("Toggle Sidebar"), menu_item("Zen Mode")]),
            ),
            section(
                "PAGINATION",
                pagination(pg.get(), 10)
                    .on_prev(move || pg.update(|p| *p = p.saturating_sub(1).max(1)))
                    .on_next(move || pg.update(|p| *p = (*p + 1).min(10))),
            ),
        ],
    )
}
