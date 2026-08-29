use pebbles::prelude::*;

use crate::ui::{screen, section};

pub fn navigation() -> impl IntoWidget {
    let tab = create_signal(0usize);
    let acc = create_signal([true, false, false]);
    let open = create_signal(false);
    let pg = create_signal(2usize);

    screen(
        "Navigation",
        "Tabs, accordion, collapsible, breadcrumb, pagination.",
        children![
            section(
                "TABS",
                tabs(tab.get())
                    .tab("Account", body("Make changes to your account here."), action(move || tab.set(0)))
                    .tab("Password", body("Change your password here."), action(move || tab.set(1)))
                    .tab("Team", body("Manage your team members."), action(move || tab.set(2))),
            ),
            section(
                "ACCORDION",
                accordion()
                    .item("Is it accessible?", muted("Yes. It follows the box protocol."), acc.get()[0], action(move || acc.update(|a| a[0] = !a[0])))
                    .item("Is it styled?", muted("Yes, from theme tokens."), acc.get()[1], action(move || acc.update(|a| a[1] = !a[1])))
                    .item("Is it animated?", muted("Not yet — on the roadmap."), acc.get()[2], action(move || acc.update(|a| a[2] = !a[2]))),
            ),
            section(
                "COLLAPSIBLE",
                collapsible("Toggle details", body("Collapsed details here."), open.get(), action(move || open.update(|v| *v = !*v))),
            ),
            section("BREADCRUMB", breadcrumb(vec!["Home".into(), "Projects".into(), "Pebbles".into()])),
            section(
                "PAGINATION",
                pagination(pg.get(), 10)
                    .on_prev(action(move || pg.update(|p| *p = p.saturating_sub(1).max(1))))
                    .on_next(action(move || pg.update(|p| *p = (*p + 1).min(10)))),
            ),
        ],
    )
}
